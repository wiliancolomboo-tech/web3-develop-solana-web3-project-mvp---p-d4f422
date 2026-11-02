import * as http from "http";
import { Connection } from "@solana/web3.js";
import { NotificationRelay } from "./relay";
import * as dotenv from "dotenv";

dotenv.config();

const PORT = process.env.PORT || 8080;
const RPC_URL = process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
const connection = new Connection(RPC_URL, "confirmed");

// Healthcheck server
const server = http.createServer(async (req, res) => {
  if (req.url === "/health" && req.method === "GET") {
    try {
      const slot = await connection.getSlot();
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "healthy", currentSlot: slot, timestamp: new Date().toISOString() }));
    } catch (err: any) {
      res.writeHead(503, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "degraded", error: err.message }));
    }
  } else {
    res.writeHead(404);
    res.end();
  }
});

server.listen(PORT, () => {
  console.log(`[HTTP] SolVibe Telemetry Health Server listening on port ${PORT}`);
});