import * as http from "http";

const PORT = process.env.PORT || 8080;

export function initHealthcheckServer(isLiveCallback: () => boolean): void {
  const server = http.createServer((req, res) => {
    if (req.url === "/health" && req.method === "GET") {
      const isHealthy = isLiveCallback();
      if (isHealthy) {
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "healthy", timestamp: Date.now() }));
      } else {
        res.writeHead(503, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "degraded", reason: "RPC connection disconnected" }));
      }
    } else {
      res.writeHead(404);
      res.end();
    }
  });

  server.listen(PORT, () => {
    console.log(`[HEALTHCHECK] Monitoring probe online on port ${PORT}`);
  });
}