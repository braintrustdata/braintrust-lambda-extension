// SCAFFOLD — not built or run in CI.
//
// The .NET SDK (Braintrust.Sdk) is OpenTelemetry-based and exports spans via OTLP to
// /otel/v1/traces, which the extension does not implement. This file exists so enabling
// .NET is a drop-in once the extension gains an OTLP passthrough route: fill in the SDK
// tracing below, add a real project (csproj) referencing Braintrust.Sdk, remove the
// "unsupported" profile in docker-compose.yml, and confirm the toolchain in the Dockerfile.
//
// Uniform contract: POST /invoke logs one marked trace + flushes; GET /health reports
// readiness. The marker must land in the span so the sink can find it.

using System.Net;
using System.Text;

var marker = Environment.GetEnvironmentVariable("BT_TEST_MARKER");
var port = Environment.GetEnvironmentVariable("APP_PORT") ?? "8080";

void HandleInvoke()
{
    // TODO(otlp): start a span with input {"prompt": marker}, then ForceFlush
    //   (OTLP → /otel/v1/traces).
    throw new NotImplementedException(".NET SDK tracing pending extension OTLP support");
}

var listener = new HttpListener();
listener.Prefixes.Add($"http://127.0.0.1:{port}/");
listener.Start();
while (true)
{
    var ctx = listener.GetContext();
    string body;
    int code = 200;
    if (ctx.Request.Url?.AbsolutePath == "/health")
    {
        body = "{\"status\":\"ok\"}";
    }
    else if (ctx.Request.Url?.AbsolutePath == "/invoke")
    {
        try { HandleInvoke(); body = "{\"ok\":true}"; }
        catch (Exception e) { code = 500; body = $"{{\"error\":\"{e.Message}\"}}"; }
    }
    else { code = 404; body = "{\"error\":\"not found\"}"; }

    var bytes = Encoding.UTF8.GetBytes(body);
    ctx.Response.StatusCode = code;
    ctx.Response.OutputStream.Write(bytes, 0, bytes.Length);
    ctx.Response.Close();
}
