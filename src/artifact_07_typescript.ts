import express, { Request, Response } from "express";
import { VibeStreamEventDispatcher } from "./agent";
import * as dotenv from "dotenv";

dotenv.config();

const app = express();
const PORT = process.env.PORT || 8080;

app.use(express.json());

let agentHealthy = false;

app.get("/health", (req: Request, res: Response) => {
  if (agentHealthy) {
    res.status(200).json({
      status: "UP",
      service: "vibestream-event-dispatcher",
      timestamp: new Date().toISOString(),
      solana_cluster: process.env.SOLANA_RPC_URL || "devnet",
    });
  } else {
    res.status(503).json({ status: "INITIALIZING" });
  }
});

app.listen(PORT, async () => {
  console.log(`🌐 Microservice running on port ${PORT}`);
  try {
    const idlPath = process.env.IDL_PATH || "./target/idl/vibestream.json";
    const keypairPath = process.env.ANCHOR_WALLET;
    const dispatcher = new VibeStreamEventDispatcher(idlPath, keypairPath);
    await dispatcher.start();
    agentHealthy = true;
    console.log("🚀 Agent dispatcher loop verified healthy.");
  } catch (err) {
    console.error("Agent startup failure:", err);
    agentHealthy = false;
  }
});