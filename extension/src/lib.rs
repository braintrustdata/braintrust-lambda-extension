use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderName, CONTENT_TYPE};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use url::Url;
use uuid::Uuid;

const EXTENSION_NAME: &str = "braintrust-lambda-extension";
const DEFAULT_MAX_PAYLOAD_BYTES: usize = 6 * 1024 * 1024;

#[derive(Clone, Debug)]
struct Settings {
    port: u16,
    spool_dir: PathBuf,
    api_key: String,
    app_url: String,
    api_url: Option<String>,
    org_name: Option<String>,
    project_id: Option<String>,
    request_timeout: Duration,
    post_runtime_timeout: Duration,
    retry_base: Duration,
    retry_max: Duration,
    fsync: bool,
    max_payload_bytes: usize,
    disable_sdk_overflow: bool,
}

impl Settings {
    fn from_env() -> Result<Self> {
        let api_key = env::var("BRAINTRUST_API_KEY")
            .context("BRAINTRUST_API_KEY is required by the Braintrust Lambda extension")?;
        if api_key.is_empty() {
            bail!("BRAINTRUST_API_KEY is required by the Braintrust Lambda extension");
        }

        let configured_api_url = env::var("BRAINTRUST_API_URL").ok();
        let api_url = env::var("BRAINTRUST_LAMBDA_UPSTREAM_URL")
            .ok()
            .or_else(|| configured_api_url.filter(|value| !is_local_url(value)));

        let configured_app_url = env::var("BRAINTRUST_APP_URL").ok();
        let app_url = env::var("BRAINTRUST_LAMBDA_UPSTREAM_APP_URL")
            .ok()
            .or_else(|| configured_app_url.filter(|value| !is_local_url(value)))
            .unwrap_or_else(|| "https://www.braintrust.dev".to_string());

        Ok(Self {
            port: env_parse("BRAINTRUST_LAMBDA_EXTENSION_PORT", 49891)?,
            spool_dir: env::var("BRAINTRUST_LAMBDA_SPOOL_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/tmp/braintrust-lambda-extension")),
            api_key,
            app_url,
            api_url,
            org_name: env::var("BRAINTRUST_ORG_NAME").ok(),
            project_id: env::var("BRAINTRUST_PROJECT_ID").ok(),
            request_timeout: env_duration("BRAINTRUST_LAMBDA_REQUEST_TIMEOUT_SECONDS", 1.5)?,
            post_runtime_timeout: env_duration(
                "BRAINTRUST_LAMBDA_POST_RUNTIME_TIMEOUT_SECONDS",
                2.0,
            )?,
            retry_base: env_duration("BRAINTRUST_LAMBDA_RETRY_BASE_SECONDS", 0.2)?,
            retry_max: env_duration("BRAINTRUST_LAMBDA_RETRY_MAX_SECONDS", 2.0)?,
            fsync: env_bool("BRAINTRUST_LAMBDA_FSYNC", true),
            max_payload_bytes: env_parse(
                "BRAINTRUST_LAMBDA_MAX_PAYLOAD_BYTES",
                DEFAULT_MAX_PAYLOAD_BYTES,
            )?,
            disable_sdk_overflow: env_bool("BRAINTRUST_LAMBDA_DISABLE_SDK_OVERFLOW", true),
        })
    }
}

fn env_parse<T>(name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| anyhow!("invalid {name}: {error}")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(anyhow!("invalid {name}: {error}")),
    }
}

fn env_duration(name: &str, default_seconds: f64) -> Result<Duration> {
    let seconds = env_parse(name, default_seconds)?;
    if !seconds.is_finite() || seconds < 0.0 {
        bail!("invalid {name}: duration must be a non-negative finite number");
    }
    Ok(Duration::from_secs_f64(seconds))
}

fn env_bool(name: &str, default: bool) -> bool {
    let Ok(value) = env::var(name) else {
        return default;
    };
    !matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "no")
}

fn is_local_url(value: &str) -> bool {
    Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1"))
}

fn join_url(base_url: &str, path: &str) -> Result<Url> {
    Url::parse(&format!("{}/", base_url.trim_end_matches('/')))
        .with_context(|| format!("invalid URL {base_url:?}"))?
        .join(path.trim_start_matches('/'))
        .with_context(|| format!("failed to join URL {base_url:?} and {path:?}"))
}

struct Spool {
    directory: PathBuf,
    fsync: bool,
}

impl Spool {
    fn new(directory: PathBuf, fsync: bool) -> Result<Self> {
        fs::create_dir_all(&directory)
            .with_context(|| format!("failed to create spool directory {}", directory.display()))?;
        Ok(Self { directory, fsync })
    }

    fn enqueue(&self, body: &[u8]) -> Result<PathBuf> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let name = format!("{timestamp}-{}.json", Uuid::new_v4().simple());
        let destination = self.directory.join(&name);
        let temporary = self.directory.join(format!(".{name}.tmp"));
        let mut output = File::create(&temporary)
            .with_context(|| format!("failed to create spool file {}", temporary.display()))?;
        output.write_all(body)?;
        output.flush()?;
        if self.fsync {
            output.sync_all()?;
        }
        drop(output);
        fs::rename(&temporary, &destination).with_context(|| {
            format!(
                "failed to move spool file {} to {}",
                temporary.display(),
                destination.display()
            )
        })?;
        Ok(destination)
    }

    fn pending(&self) -> Result<Vec<PathBuf>> {
        let mut entries = fs::read_dir(&self.directory)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect::<Vec<_>>();
        entries.sort();
        Ok(entries)
    }

    fn delete(&self, path: &Path) -> Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

struct LoginDiscovery {
    settings: Arc<Settings>,
    response: Mutex<Option<Value>>,
    client: Client,
}

impl LoginDiscovery {
    fn new(settings: Arc<Settings>) -> Result<Self> {
        let client = Client::builder()
            .timeout(settings.request_timeout)
            .user_agent(EXTENSION_NAME)
            .build()?;
        Ok(Self {
            settings,
            response: Mutex::new(None),
            client,
        })
    }

    fn response(&self) -> Result<Value> {
        let mut cached = self.response.lock().unwrap();
        if let Some(response) = cached.as_ref() {
            return Ok(response.clone());
        }
        let response = self.fetch()?;
        *cached = Some(response.clone());
        Ok(response)
    }

    fn fetch(&self) -> Result<Value> {
        let url = join_url(&self.settings.app_url, "/api/apikey/login")?;
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.settings.api_key)
            .header(CONTENT_TYPE, "application/json")
            .body(Vec::new())
            .send()?
            .error_for_status()?
            .json::<Value>()?;
        let orgs = response
            .get("org_info")
            .and_then(Value::as_array)
            .filter(|orgs| !orgs.is_empty())
            .context("Braintrust login returned no organizations")?;
        if orgs.iter().any(|org| !org.is_object()) {
            bail!("Braintrust login returned invalid organization metadata");
        }
        Ok(response)
    }

    fn selected_org(&self) -> Result<Value> {
        let response = self.response()?;
        let orgs = response
            .get("org_info")
            .and_then(Value::as_array)
            .context("Braintrust login returned invalid organization metadata")?;
        if let Some(org_name) = self.settings.org_name.as_deref() {
            return orgs
                .iter()
                .find(|org| org.get("name").and_then(Value::as_str) == Some(org_name))
                .cloned()
                .with_context(|| format!("Braintrust organization {org_name:?} was not found"));
        }
        orgs.first()
            .cloned()
            .context("Braintrust login returned no organizations")
    }

    fn api_url(&self) -> Result<String> {
        self.selected_org()?
            .get("api_url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .map(str::to_owned)
            .context("Braintrust login returned an invalid data-plane URL")
    }

    fn local_response(&self, local_url: &str) -> Result<Value> {
        let mut response = self.response()?;
        let orgs = response
            .get_mut("org_info")
            .and_then(Value::as_array_mut)
            .context("Braintrust login returned invalid organization metadata")?;
        for org in orgs {
            org.as_object_mut()
                .context("Braintrust login returned invalid organization metadata")?
                .insert("api_url".to_string(), Value::String(local_url.to_string()));
        }
        Ok(response)
    }

    fn prefetch(self: &Arc<Self>) -> Result<()> {
        let discovery = Arc::clone(self);
        thread::Builder::new()
            .name("braintrust-login".to_string())
            .spawn(move || {
                if let Err(error) = discovery.response() {
                    eprintln!("Braintrust Lambda login prefetch failed: {error:#}");
                }
            })
            .context("failed to start Braintrust login thread")?;
        Ok(())
    }
}

trait LogSender: Send + Sync {
    fn send(&self, body: &[u8]) -> Result<()>;
}

struct UpstreamSender {
    settings: Arc<Settings>,
    discovery: Arc<LoginDiscovery>,
    api_url: Mutex<Option<String>>,
    client: Client,
}

impl UpstreamSender {
    fn new(settings: Arc<Settings>, discovery: Arc<LoginDiscovery>) -> Result<Self> {
        let client = Client::builder()
            .timeout(settings.request_timeout)
            .user_agent(EXTENSION_NAME)
            .build()?;
        Ok(Self {
            api_url: Mutex::new(settings.api_url.clone()),
            settings,
            discovery,
            client,
        })
    }

    fn api_url(&self) -> Result<String> {
        let mut api_url = self.api_url.lock().unwrap();
        if let Some(api_url) = api_url.as_ref() {
            return Ok(api_url.clone());
        }
        let discovered = self.discovery.api_url()?;
        *api_url = Some(discovered.clone());
        Ok(discovered)
    }
}

impl LogSender for UpstreamSender {
    fn send(&self, body: &[u8]) -> Result<()> {
        let url = join_url(&self.api_url()?, "/logs3")?;
        self.client
            .post(url)
            .bearer_auth(&self.settings.api_key)
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_vec())
            .send()?
            .error_for_status()?
            .bytes()?;
        Ok(())
    }
}

#[derive(Default)]
struct WorkerState {
    stopping: bool,
    in_flight: bool,
}

struct DeliveryWorker {
    spool: Arc<Spool>,
    sender: Arc<dyn LogSender>,
    retry_base: Duration,
    retry_max: Duration,
    state: Mutex<WorkerState>,
    condition: Condvar,
}

impl DeliveryWorker {
    fn new(
        spool: Arc<Spool>,
        sender: Arc<dyn LogSender>,
        retry_base: Duration,
        retry_max: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            spool,
            sender,
            retry_base,
            retry_max,
            state: Mutex::new(WorkerState::default()),
            condition: Condvar::new(),
        })
    }

    fn start(self: &Arc<Self>) -> Result<WorkerRunner> {
        let worker = Arc::clone(self);
        let thread = thread::Builder::new()
            .name("braintrust-delivery".to_string())
            .spawn(move || worker.run())?;
        Ok(WorkerRunner {
            worker: Arc::clone(self),
            thread: Some(thread),
        })
    }

    fn notify(&self) {
        let _state = self.state.lock().unwrap();
        self.condition.notify_all();
    }

    fn wait_until_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap();
        loop {
            let has_pending = self
                .spool
                .pending()
                .map(|pending| !pending.is_empty())
                .unwrap_or(true);
            if !state.in_flight && !has_pending {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (next_state, result) = self
                .condition
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap();
            state = next_state;
            if result.timed_out() {
                return false;
            }
        }
    }

    fn stop(&self) {
        let mut state = self.state.lock().unwrap();
        state.stopping = true;
        self.condition.notify_all();
    }

    fn run(&self) {
        let mut retry = self.retry_base;
        loop {
            if self.state.lock().unwrap().stopping {
                return;
            }
            let pending = match self.spool.pending() {
                Ok(pending) => pending,
                Err(error) => {
                    eprintln!("Braintrust Lambda spool scan failed: {error:#}");
                    if self.wait_for_retry(retry) {
                        return;
                    }
                    retry = next_retry(retry, self.retry_max);
                    continue;
                }
            };
            let Some(path) = pending.first() else {
                let mut state = self.state.lock().unwrap();
                let still_empty = self
                    .spool
                    .pending()
                    .map(|pending| pending.is_empty())
                    .unwrap_or(false);
                if !still_empty {
                    continue;
                }
                state.in_flight = false;
                self.condition.notify_all();
                if state.stopping {
                    return;
                }
                drop(self.condition.wait(state).unwrap());
                retry = self.retry_base;
                continue;
            };

            {
                let mut state = self.state.lock().unwrap();
                state.in_flight = true;
            }
            let result = fs::read(path)
                .with_context(|| format!("failed to read spool file {}", path.display()))
                .and_then(|body| self.sender.send(&body))
                .and_then(|()| self.spool.delete(path));
            match result {
                Ok(()) => retry = self.retry_base,
                Err(error) => {
                    eprintln!("Braintrust Lambda delivery failed: {error:#}");
                    let stopping = self.wait_for_retry(retry);
                    if stopping {
                        let mut state = self.state.lock().unwrap();
                        state.in_flight = false;
                        self.condition.notify_all();
                        return;
                    }
                    retry = next_retry(retry, self.retry_max);
                }
            }
            let mut state = self.state.lock().unwrap();
            state.in_flight = false;
            self.condition.notify_all();
        }
    }

    fn wait_for_retry(&self, timeout: Duration) -> bool {
        let state = self.state.lock().unwrap();
        if state.stopping {
            return true;
        }
        let (state, _) = self.condition.wait_timeout(state, timeout).unwrap();
        state.stopping
    }
}

fn next_retry(current: Duration, maximum: Duration) -> Duration {
    current.saturating_mul(2).min(maximum)
}

struct WorkerRunner {
    worker: Arc<DeliveryWorker>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WorkerRunner {
    fn drop(&mut self) {
        self.worker.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct RuntimeDoneTracker {
    completed: Mutex<HashSet<String>>,
    condition: Condvar,
}

impl RuntimeDoneTracker {
    fn record(&self, request_id: String) {
        self.completed.lock().unwrap().insert(request_id);
        self.condition.notify_all();
    }

    fn wait(&self, request_id: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut completed = self.completed.lock().unwrap();
        loop {
            if completed.remove(request_id) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (next_completed, result) = self
                .condition
                .wait_timeout(completed, deadline.saturating_duration_since(now))
                .unwrap();
            completed = next_completed;
            if result.timed_out() {
                return false;
            }
        }
    }
}

struct LocalService {
    settings: Arc<Settings>,
    spool: Arc<Spool>,
    worker: Arc<DeliveryWorker>,
    runtime_done: Arc<RuntimeDoneTracker>,
    discovery: Arc<LoginDiscovery>,
    bound_port: AtomicU16,
}

impl LocalService {
    fn local_url(&self) -> String {
        let port = match self.bound_port.load(Ordering::Relaxed) {
            0 => self.settings.port,
            port => port,
        };
        format!("http://127.0.0.1:{port}")
    }

    fn accept_logs(&self, body: &[u8]) -> Result<()> {
        self.spool.enqueue(body)?;
        self.worker.notify();
        Ok(())
    }

    fn accept_telemetry(&self, events: &Value) {
        let Some(events) = events.as_array() else {
            return;
        };
        for event in events {
            if event.get("type").and_then(Value::as_str) != Some("platform.runtimeDone") {
                continue;
            }
            if let Some(request_id) = event
                .get("record")
                .and_then(|record| record.get("requestId"))
                .and_then(Value::as_str)
            {
                self.runtime_done.record(request_id.to_string());
            }
        }
    }
}

type HttpResponse = Response<Cursor<Vec<u8>>>;

enum HttpFailure {
    BadRequest(String),
    Internal(anyhow::Error),
}

fn json_response(status: u16, value: Value) -> HttpResponse {
    let body = serde_json::to_vec(&value).expect("JSON response serialization failed");
    Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(
            Header::from_bytes("Content-Type", "application/json")
                .expect("static content-type header is valid"),
        )
}

fn read_body(request: &mut Request, max_bytes: usize) -> std::result::Result<Vec<u8>, HttpFailure> {
    if request
        .body_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err(HttpFailure::BadRequest(format!(
            "payload exceeds {max_bytes} bytes"
        )));
    }
    let mut body = Vec::new();
    request
        .as_reader()
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| HttpFailure::Internal(error.into()))?;
    if body.len() > max_bytes {
        return Err(HttpFailure::BadRequest(format!(
            "payload exceeds {max_bytes} bytes"
        )));
    }
    Ok(body)
}

fn route_request(
    request: &mut Request,
    service: &LocalService,
) -> std::result::Result<HttpResponse, HttpFailure> {
    let parsed_url = Url::parse(&format!("http://localhost{}", request.url()))
        .map_err(|error| HttpFailure::BadRequest(error.to_string()))?;
    let path = parsed_url.path();
    let method = request.method().clone();

    if method == Method::Get && path == "/health" {
        return Ok(json_response(200, json!({"status": "ok"})));
    }
    if method == Method::Get && path == "/version" {
        let response = if service.settings.disable_sdk_overflow {
            json!({})
        } else {
            json!({"logs3_payload_max_bytes": service.settings.max_payload_bytes})
        };
        return Ok(json_response(200, response));
    }
    if method == Method::Get && path == "/api/project" {
        let project_id = parsed_url
            .query_pairs()
            .find_map(|(key, value)| (key == "id" && !value.is_empty()).then(|| value.into_owned()))
            .ok_or_else(|| HttpFailure::BadRequest("project id is required".to_string()))?;
        return Ok(json_response(
            200,
            json!({"id": project_id, "name": Value::Null}),
        ));
    }
    if method == Method::Post && path == "/api/apikey/login" {
        read_body(request, service.settings.max_payload_bytes)?;
        let response = service
            .discovery
            .local_response(&service.local_url())
            .map_err(HttpFailure::Internal)?;
        return Ok(json_response(200, response));
    }
    if method == Method::Post && path == "/api/project/register" {
        let body = read_body(request, service.settings.max_payload_bytes)?;
        let request: Value = serde_json::from_slice(&body)
            .map_err(|error| HttpFailure::BadRequest(error.to_string()))?;
        let request = request.as_object().ok_or_else(|| {
            HttpFailure::BadRequest("project registration body must be an object".to_string())
        })?;
        let project_id = service.settings.project_id.as_ref().ok_or_else(|| {
            HttpFailure::BadRequest(
                "BRAINTRUST_PROJECT_ID is required when init_logger() does not receive project_id"
                    .to_string(),
            )
        })?;
        return Ok(json_response(
            200,
            json!({
                "project": {
                    "id": project_id,
                    "name": request.get("project_name").cloned().unwrap_or(Value::Null),
                }
            }),
        ));
    }
    if method == Method::Post && path == "/logs3" {
        let body = read_body(request, service.settings.max_payload_bytes)?;
        service.accept_logs(&body).map_err(HttpFailure::Internal)?;
        return Ok(json_response(200, json!({"ids": [], "xact_id": null})));
    }
    if method == Method::Post && path == "/telemetry" {
        let body = read_body(request, service.settings.max_payload_bytes)?;
        let events = serde_json::from_slice(&body)
            .map_err(|error| HttpFailure::BadRequest(error.to_string()))?;
        service.accept_telemetry(&events);
        return Ok(json_response(200, json!({"status": "ok"})));
    }
    Ok(json_response(404, json!({"error": "not found"})))
}

fn handle_request(mut request: Request, service: &LocalService) {
    let response = match route_request(&mut request, service) {
        Ok(response) => response,
        Err(HttpFailure::BadRequest(message)) => json_response(400, json!({"error": message})),
        Err(HttpFailure::Internal(error)) => {
            eprintln!("Braintrust Lambda local intake failed: {error:#}");
            json_response(500, json!({"error": "local intake failed"}))
        }
    };
    if let Err(error) = request.respond(response) {
        eprintln!("Braintrust Lambda local response failed: {error}");
    }
}

struct LocalServer {
    port: u16,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LocalServer {
    fn start(service: Arc<LocalService>) -> Result<Self> {
        let server = Server::http(("0.0.0.0", service.settings.port))
            .map_err(|error| anyhow!("failed to start local HTTP server: {error}"))?;
        let port = server
            .server_addr()
            .to_ip()
            .context("local HTTP server did not bind an IP socket")?
            .port();
        service.bound_port.store(port, Ordering::Relaxed);
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_stopping = Arc::clone(&stopping);
        let thread = thread::Builder::new()
            .name("braintrust-local-http".to_string())
            .spawn(move || {
                while !thread_stopping.load(Ordering::Relaxed) {
                    match server.recv_timeout(Duration::from_millis(50)) {
                        Ok(Some(request)) => handle_request(request, &service),
                        Ok(None) => {}
                        Err(error) => {
                            eprintln!("Braintrust Lambda local server failed: {error}");
                            return;
                        }
                    }
                }
            })?;
        Ok(Self {
            port,
            stopping,
            thread: Some(thread),
        })
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct LambdaExtensionsClient {
    base_url: String,
    port: u16,
    extension_id: Option<String>,
    client: Client,
}

impl LambdaExtensionsClient {
    fn new(runtime_api: &str, port: u16) -> Result<Self> {
        Ok(Self {
            base_url: format!("http://{runtime_api}"),
            port,
            extension_id: None,
            client: Client::builder().timeout(None).build()?,
        })
    }

    fn register(&mut self) -> Result<()> {
        let response = self
            .client
            .post(join_url(&self.base_url, "/2020-01-01/extension/register")?)
            .header("Lambda-Extension-Name", EXTENSION_NAME)
            .json(&json!({"events": ["INVOKE", "SHUTDOWN"]}))
            .send()?
            .error_for_status()?;
        let header = HeaderName::from_static("lambda-extension-identifier");
        let extension_id = response
            .headers()
            .get(header)
            .context("Lambda did not return an extension identifier")?
            .to_str()?
            .to_string();
        response.bytes()?;
        self.extension_id = Some(extension_id);
        Ok(())
    }

    fn subscribe_telemetry(&self) -> Result<()> {
        let extension_id = self
            .extension_id
            .as_ref()
            .context("the extension must register before subscribing to telemetry")?;
        let response = self
            .client
            .put(join_url(&self.base_url, "/2022-07-01/telemetry")?)
            .header("Lambda-Extension-Identifier", extension_id)
            .json(&json!({
                "schemaVersion": "2022-12-13",
                "types": ["platform"],
                "buffering": {
                    "maxItems": 1000,
                    "maxBytes": 262144,
                    "timeoutMs": 25,
                },
                "destination": {
                    "protocol": "HTTP",
                    "URI": format!("http://sandbox.localdomain:{}/telemetry", self.port),
                },
            }))
            .send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            bail!("Lambda Telemetry API returned {status}: {body}");
        }
        Ok(())
    }

    fn next_event(&self) -> Result<Value> {
        let extension_id = self
            .extension_id
            .as_ref()
            .context("the extension must register before requesting events")?;
        Ok(self
            .client
            .get(join_url(
                &self.base_url,
                "/2020-01-01/extension/event/next",
            )?)
            .header("Lambda-Extension-Identifier", extension_id)
            .send()?
            .error_for_status()?
            .json()?)
    }
}

fn remaining(deadline_ms: Option<f64>) -> Duration {
    let Some(deadline_ms) = deadline_ms else {
        return Duration::ZERO;
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0;
    Duration::from_secs_f64(((deadline_ms - now_ms) / 1000.0 - 0.05).max(0.0))
}

pub fn run() -> Result<()> {
    let settings = Arc::new(Settings::from_env()?);
    let spool = Arc::new(Spool::new(settings.spool_dir.clone(), settings.fsync)?);
    let discovery = Arc::new(LoginDiscovery::new(Arc::clone(&settings))?);
    let sender = Arc::new(UpstreamSender::new(
        Arc::clone(&settings),
        Arc::clone(&discovery),
    )?);
    let worker = DeliveryWorker::new(
        Arc::clone(&spool),
        sender,
        settings.retry_base,
        settings.retry_max,
    );
    let runtime_done = Arc::new(RuntimeDoneTracker::default());
    let service = Arc::new(LocalService {
        settings: Arc::clone(&settings),
        spool,
        worker: Arc::clone(&worker),
        runtime_done: Arc::clone(&runtime_done),
        discovery: Arc::clone(&discovery),
        bound_port: AtomicU16::new(0),
    });
    let local_server = LocalServer::start(service)?;
    discovery.prefetch()?;
    let worker_runner = worker.start()?;

    let runtime_api = env::var("AWS_LAMBDA_RUNTIME_API")
        .context("AWS_LAMBDA_RUNTIME_API is required by the Braintrust Lambda extension")?;
    let mut client = LambdaExtensionsClient::new(&runtime_api, local_server.port)?;
    client.register()?;
    client.subscribe_telemetry()?;

    loop {
        let event = client.next_event()?;
        match event.get("eventType").and_then(Value::as_str) {
            Some("INVOKE") => {
                let invoke_remaining = remaining(event.get("deadlineMs").and_then(Value::as_f64));
                if let Some(request_id) = event.get("requestId").and_then(Value::as_str) {
                    runtime_done.wait(request_id, invoke_remaining);
                }
                let flush_timeout = settings
                    .post_runtime_timeout
                    .min(remaining(event.get("deadlineMs").and_then(Value::as_f64)));
                worker.wait_until_idle(flush_timeout);
            }
            Some("SHUTDOWN") => {
                worker.wait_until_idle(remaining(event.get("deadlineMs").and_then(Value::as_f64)));
                drop(worker_runner);
                drop(local_server);
                return Ok(());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, Receiver};
    use tempfile::TempDir;

    #[derive(Default)]
    struct RecordingSender {
        bodies: Mutex<Vec<Vec<u8>>>,
    }

    impl LogSender for RecordingSender {
        fn send(&self, body: &[u8]) -> Result<()> {
            self.bodies.lock().unwrap().push(body.to_vec());
            Ok(())
        }
    }

    fn settings(temp_dir: &TempDir) -> Arc<Settings> {
        Arc::new(Settings {
            port: 0,
            spool_dir: temp_dir.path().to_path_buf(),
            api_key: "test-api-key".to_string(),
            app_url: "http://unused.invalid".to_string(),
            api_url: Some("http://unused.invalid".to_string()),
            org_name: None,
            project_id: Some("project-123".to_string()),
            request_timeout: Duration::from_secs(1),
            post_runtime_timeout: Duration::from_secs(1),
            retry_base: Duration::from_millis(10),
            retry_max: Duration::from_millis(20),
            fsync: false,
            max_payload_bytes: 1024 * 1024,
            disable_sdk_overflow: true,
        })
    }

    fn service(
        settings: Arc<Settings>,
        sender: Arc<dyn LogSender>,
    ) -> Result<(Arc<LocalService>, WorkerRunner)> {
        let spool = Arc::new(Spool::new(settings.spool_dir.clone(), settings.fsync)?);
        let discovery = Arc::new(LoginDiscovery::new(Arc::clone(&settings))?);
        let worker = DeliveryWorker::new(
            Arc::clone(&spool),
            sender,
            settings.retry_base,
            settings.retry_max,
        );
        let runner = worker.start()?;
        Ok((
            Arc::new(LocalService {
                settings,
                spool,
                worker,
                runtime_done: Arc::new(RuntimeDoneTracker::default()),
                discovery,
                bound_port: AtomicU16::new(0),
            }),
            runner,
        ))
    }

    type RecordedRequest = (String, Vec<u8>, Option<String>);

    fn start_upstream(
        request_count: usize,
    ) -> Result<(String, Receiver<RecordedRequest>, JoinHandle<Result<()>>)> {
        let server = Server::http("127.0.0.1:0")
            .map_err(|error| anyhow!("failed to start upstream server: {error}"))?;
        let port = server
            .server_addr()
            .to_ip()
            .context("upstream server did not bind an IP socket")?
            .port();
        let base_url = format!("http://127.0.0.1:{port}");
        let response_api_url = base_url.clone();
        let (sender, receiver) = mpsc::channel();
        let thread = thread::spawn(move || -> Result<()> {
            for _ in 0..request_count {
                let mut request = server
                    .recv_timeout(Duration::from_secs(2))?
                    .context("timed out waiting for upstream request")?;
                let path = request.url().to_string();
                let authorization = request
                    .headers()
                    .iter()
                    .find(|header| header.field.equiv("Authorization"))
                    .map(|header| header.value.as_str().to_string());
                let mut body = Vec::new();
                request.as_reader().read_to_end(&mut body)?;
                sender.send((path.clone(), body, authorization))?;
                let response = match path.as_str() {
                    "/api/apikey/login" => json!({
                        "org_info": [{
                            "id": "org-123",
                            "name": "example-org",
                            "api_url": response_api_url,
                        }]
                    }),
                    "/logs3" => json!({}),
                    _ => bail!("unexpected upstream request path {path}"),
                };
                request.respond(json_response(200, response))?;
            }
            Ok(())
        });
        Ok((base_url, receiver, thread))
    }

    #[test]
    fn local_logs_endpoint_spools_and_delivers() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let settings = settings(&temp_dir);
        let sender = Arc::new(RecordingSender::default());
        let (service, _runner) = service(settings, sender.clone())?;
        let server = LocalServer::start(Arc::clone(&service))?;
        let payload = br#"{"rows":[],"api_version":2}"#;

        let response = Client::new()
            .post(format!("http://127.0.0.1:{}/logs3", server.port))
            .body(payload.to_vec())
            .send()?;

        assert_eq!(response.status(), 200);
        assert!(service.worker.wait_until_idle(Duration::from_secs(1)));
        assert_eq!(*sender.bodies.lock().unwrap(), vec![payload.to_vec()]);
        assert!(service.spool.pending()?.is_empty());
        Ok(())
    }

    #[test]
    fn version_endpoint_disables_sdk_overflow_by_default() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let settings = settings(&temp_dir);
        let (service, _runner) = service(settings, Arc::new(RecordingSender::default()))?;
        let server = LocalServer::start(service)?;

        let response = Client::new()
            .get(format!("http://127.0.0.1:{}/version", server.port))
            .send()?
            .json::<Value>()?;

        assert_eq!(response, json!({}));
        Ok(())
    }

    #[test]
    fn version_endpoint_can_advertise_local_limit() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut settings = (*settings(&temp_dir)).clone();
        settings.max_payload_bytes = 123_456;
        settings.disable_sdk_overflow = false;
        let (service, _runner) = service(Arc::new(settings), Arc::new(RecordingSender::default()))?;
        let server = LocalServer::start(service)?;

        let response = Client::new()
            .get(format!("http://127.0.0.1:{}/version", server.port))
            .send()?
            .json::<Value>()?;

        assert_eq!(response, json!({"logs3_payload_max_bytes": 123456}));
        Ok(())
    }

    #[test]
    fn runtime_done_telemetry_releases_waiter() {
        let tracker = RuntimeDoneTracker::default();
        tracker.record("request-123".to_string());
        assert!(tracker.wait("request-123", Duration::ZERO));
    }

    #[test]
    fn discovers_data_plane_and_sends_logs_with_api_key() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let (upstream_url, requests, upstream_thread) = start_upstream(2)?;
        let mut settings = (*settings(&temp_dir)).clone();
        settings.app_url = upstream_url;
        settings.api_url = None;
        settings.org_name = Some("example-org".to_string());
        let settings = Arc::new(settings);
        let discovery = Arc::new(LoginDiscovery::new(Arc::clone(&settings))?);
        let sender = UpstreamSender::new(settings, discovery)?;
        let payload = br#"{"rows":[],"api_version":2}"#;

        sender.send(payload)?;

        let requests = requests.iter().take(2).collect::<Vec<_>>();
        assert_eq!(
            requests
                .iter()
                .map(|request| request.0.as_str())
                .collect::<Vec<_>>(),
            vec!["/api/apikey/login", "/logs3"]
        );
        assert!(requests
            .iter()
            .all(|request| request.2.as_deref() == Some("Bearer test-api-key")));
        assert_eq!(requests[1].1, payload);
        upstream_thread
            .join()
            .map_err(|_| anyhow!("upstream server thread panicked"))??;
        Ok(())
    }

    #[test]
    fn local_login_and_project_routes_preserve_sdk_contract() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let (upstream_url, requests, upstream_thread) = start_upstream(1)?;
        let mut settings = (*settings(&temp_dir)).clone();
        settings.app_url = upstream_url;
        settings.api_url = None;
        settings.org_name = Some("example-org".to_string());
        let settings = Arc::new(settings);
        let (service, _runner) = service(settings, Arc::new(RecordingSender::default()))?;
        let server = LocalServer::start(service)?;
        let client = Client::new();
        let local_url = format!("http://127.0.0.1:{}", server.port);

        let login = client
            .post(format!("{local_url}/api/apikey/login"))
            .body(Vec::new())
            .send()?
            .error_for_status()?
            .json::<Value>()?;
        assert_eq!(login["org_info"][0]["id"], "org-123");
        assert_eq!(login["org_info"][0]["name"], "example-org");
        assert_eq!(login["org_info"][0]["api_url"], local_url);

        let project = client
            .get(format!("{local_url}/api/project?id=project-123"))
            .send()?
            .error_for_status()?
            .json::<Value>()?;
        assert_eq!(project, json!({"id": "project-123", "name": null}));

        let registration = client
            .post(format!("{local_url}/api/project/register"))
            .json(&json!({"project_name": "gen-search"}))
            .send()?
            .error_for_status()?
            .json::<Value>()?;
        assert_eq!(
            registration,
            json!({"project": {"id": "project-123", "name": "gen-search"}})
        );
        assert_eq!(
            requests.recv_timeout(Duration::from_secs(1))?.0,
            "/api/apikey/login"
        );
        upstream_thread
            .join()
            .map_err(|_| anyhow!("upstream server thread panicked"))??;
        Ok(())
    }

    #[test]
    fn registers_and_subscribes_with_lambda_extensions_api() -> Result<()> {
        let server = Server::http("127.0.0.1:0")
            .map_err(|error| anyhow!("failed to start Lambda API server: {error}"))?;
        let port = server
            .server_addr()
            .to_ip()
            .context("Lambda API server did not bind an IP socket")?
            .port();
        let thread = thread::spawn(move || -> Result<()> {
            let mut register = server
                .recv_timeout(Duration::from_secs(2))?
                .context("timed out waiting for extension registration")?;
            assert_eq!(register.method(), &Method::Post);
            assert_eq!(register.url(), "/2020-01-01/extension/register");
            assert!(register.headers().iter().any(|header| {
                header.field.equiv("Lambda-Extension-Name")
                    && header.value.as_str() == EXTENSION_NAME
            }));
            let mut register_body = Vec::new();
            register.as_reader().read_to_end(&mut register_body)?;
            assert_eq!(
                serde_json::from_slice::<Value>(&register_body)?,
                json!({"events": ["INVOKE", "SHUTDOWN"]})
            );
            register.respond(
                json_response(200, json!({})).with_header(
                    Header::from_bytes("Lambda-Extension-Identifier", "extension-123")
                        .expect("static extension header is valid"),
                ),
            )?;

            let mut telemetry = server
                .recv_timeout(Duration::from_secs(2))?
                .context("timed out waiting for telemetry subscription")?;
            assert_eq!(telemetry.method(), &Method::Put);
            assert_eq!(telemetry.url(), "/2022-07-01/telemetry");
            let mut telemetry_body = Vec::new();
            telemetry.as_reader().read_to_end(&mut telemetry_body)?;
            let telemetry_body = serde_json::from_slice::<Value>(&telemetry_body)?;
            assert_eq!(telemetry_body["schemaVersion"], "2022-12-13");
            assert_eq!(telemetry_body["buffering"]["maxItems"], 1000);
            assert_eq!(
                telemetry_body["destination"]["URI"],
                format!("http://sandbox.localdomain:{port}/telemetry")
            );
            telemetry.respond(json_response(200, json!({})))?;

            let next = server
                .recv_timeout(Duration::from_secs(2))?
                .context("timed out waiting for next-event request")?;
            assert_eq!(next.method(), &Method::Get);
            assert_eq!(next.url(), "/2020-01-01/extension/event/next");
            next.respond(json_response(200, json!({"eventType": "SHUTDOWN"})))?;
            Ok(())
        });

        let mut client = LambdaExtensionsClient::new(&format!("127.0.0.1:{port}"), port)?;
        client.register()?;
        client.subscribe_telemetry()?;
        assert_eq!(client.next_event()?["eventType"], "SHUTDOWN");
        thread
            .join()
            .map_err(|_| anyhow!("Lambda API server thread panicked"))??;
        Ok(())
    }
}
