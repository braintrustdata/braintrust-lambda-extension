// SCAFFOLD — not built or run in CI.
//
// The Java SDK (dev.braintrust:braintrust-sdk-java) is OpenTelemetry-based and exports
// spans via OTLP to /otel/v1/traces, which the extension does not implement. This file
// exists so enabling Java is a drop-in once the extension gains an OTLP passthrough
// route: fill in the SDK tracing below, add a real build (Gradle/Maven) with the SDK
// dependency, remove the "unsupported" profile in docker-compose.yml, and confirm the
// toolchain in the Dockerfile.
//
// Uniform contract: POST /invoke logs one marked trace + flushes; GET /health reports
// readiness. The marker must land in the span so the sink can find it.

import com.sun.net.httpserver.HttpServer;
import java.net.InetSocketAddress;

public class app {
    static final String MARKER = System.getenv("BT_TEST_MARKER");

    static void handleInvoke() {
        // TODO(otlp): start a span with input {"prompt": MARKER}, then force-flush
        //   (OTLP → /otel/v1/traces).
        throw new UnsupportedOperationException("Java SDK tracing pending extension OTLP support");
    }

    public static void main(String[] args) throws Exception {
        int port = Integer.parseInt(System.getenv().getOrDefault("APP_PORT", "8080"));
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", port), 0);
        server.createContext("/health", ex -> respond(ex, 200, "{\"status\":\"ok\"}"));
        server.createContext("/invoke", ex -> {
            try {
                handleInvoke();
                respond(ex, 200, "{\"ok\":true}");
            } catch (RuntimeException e) {
                respond(ex, 500, "{\"error\":\"" + e.getMessage() + "\"}");
            }
        });
        server.start();
    }

    static void respond(com.sun.net.httpserver.HttpExchange ex, int code, String body) throws java.io.IOException {
        byte[] bytes = body.getBytes();
        ex.sendResponseHeaders(code, bytes.length);
        ex.getResponseBody().write(bytes);
        ex.close();
    }
}
