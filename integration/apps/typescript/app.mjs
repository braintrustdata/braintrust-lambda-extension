// TypeScript/JS SDK test app: a Lambda-handler stand-in.
//
// POST /invoke logs one marked mock-LLM trace via the Braintrust SDK and flushes.
// GET /health reports readiness. The unique marker lands in the span's input so the
// harness can find it in the /logs3 payload the extension forwards to the sink.

import http from "node:http";
import { initLogger, flush } from "braintrust";

const MARKER = process.env.BT_TEST_MARKER;
const PROJECT_ID = process.env.BRAINTRUST_PROJECT_ID || "test-project";
const PORT = parseInt(process.env.APP_PORT || "8080", 10);

const logger = initLogger({ projectId: PROJECT_ID });

async function handleInvoke() {
  await logger.traced(
    async (span) => {
      span.log({
        input: { prompt: MARKER },
        output: { completion: "hello from the mock LLM" },
      });
    },
    { name: "request" },
  );
  await flush();
}

const server = http.createServer(async (req, res) => {
  const json = (code, obj) => {
    res.writeHead(code, { "Content-Type": "application/json" });
    res.end(JSON.stringify(obj));
  };
  // Drain the request body.
  for await (const _chunk of req) {
    /* ignore */
  }
  if (req.method === "GET" && req.url === "/health") {
    return json(200, { status: "ok" });
  }
  if (req.method === "POST" && req.url === "/invoke") {
    try {
      await handleInvoke();
      return json(200, { ok: true });
    } catch (err) {
      return json(500, { error: String(err) });
    }
  }
  json(404, { error: "not found" });
});

server.listen(PORT, "127.0.0.1");
