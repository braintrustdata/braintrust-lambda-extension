// SCAFFOLD — not built or run in CI.
//
// The Go SDK (github.com/braintrustdata/braintrust-sdk-go) is OpenTelemetry-based and
// exports spans via OTLP to /otel/v1/traces, which the extension does not implement.
// This file exists so enabling Go is a drop-in once the extension gains an OTLP
// passthrough route: fill in the SDK tracing below, remove the "unsupported" profile in
// docker-compose.yml, and confirm the toolchain in the Dockerfile.
//
// Uniform contract: POST /invoke logs one marked trace + flushes; GET /health reports
// readiness. The marker must land in the span so the sink can find it.

package main

import (
	"fmt"
	"net/http"
	"os"
)

var marker = os.Getenv("BT_TEST_MARKER")

func handleInvoke() error {
	// TODO(otlp): start a span with input {"prompt": marker}, then ForceFlush
	//   (OTLP → /otel/v1/traces).
	return fmt.Errorf("go SDK tracing pending extension OTLP support")
}

func main() {
	port := os.Getenv("APP_PORT")
	if port == "" {
		port = "8080"
	}
	http.HandleFunc("/health", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `{"status":"ok"}`)
	})
	http.HandleFunc("/invoke", func(w http.ResponseWriter, _ *http.Request) {
		if err := handleInvoke(); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `{"ok":true}`)
	})
	_ = http.ListenAndServe("127.0.0.1:"+port, nil)
}
