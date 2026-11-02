import { Command } from "commander";
import * as web3 from "@solana/web3.js";
import * as fs from "fs";
import dotenv from "dotenv";

dotenv.config();

const program = new Command();
program
  .name("vibestream-cli")
  .description("VibeStream Protocol Admin and Operator Automation Interface")
  .version("1.0.0");

function loadKeypair(path: string): web3.Keypair {
  const secretKey = JSON.parse(fs.readFileSync(path, "utf-8"));
  return web3.Keypair.fromSecretKey(new Uint8Array(secretKey));
}

program
  .command("status")
  .description("Query health and current block height of configured Solana cluster")
  .action(async () => {
    const rpcUrl = process.env.SOLANA_RPC_URL || "http://127.0.0.1:8899";
    const conn = new web3.Connection(rpcUrl, "confirmed");
    const slot = await conn.getSlot();
    const version = await conn.getVersion();
    console.log(`Cluster: ${rpcUrl}`);
    console.log(`Solana Node Version: ${version["solana-core"]}`);
    console.log(`Current Slot: ${slot}`);
  });

program
  .command("derive-pdas")
  .description("Derive Grant State and Escrow Vault PDA addresses")
  .requiredOption("-s, --sponsor <pubkey>", "Sponsor Public Key")
  .requiredOption("-i, --grant-id <number>", "Numeric Grant ID")
  .action(async (options) => {
    const programId = new web3.PublicKey(process.env.PROGRAM_ID || "V1beS1ream111111111111111111111111111111111");
    const sponsorKey = new web3.PublicKey(options.sponsor);
    const grantIdBuf = Buffer.alloc(8);
    grantIdBuf.writeBigUInt64LE(BigInt(options.grantId));

    const [grantPda, grantBump] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), sponsorKey.toBuffer(), grantIdBuf],
      programId
    );

    const [vaultPda, vaultBump] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), sponsorKey.toBuffer(), grantIdBuf],
      programId
    );

    console.log("\n=== DERIVED PROGRAM DERIVED ADDRESSES ===");
    console.log(`Grant State PDA: ${grantPda.toBase58()} (Bump: ${grantBump})`);
    console.log(`Vault Token PDA: ${vaultPda.toBase58()} (Bump: ${vaultBump})\n`);
  });

program.parse(process.argv);