import * as anchor from "@coral-xyz/anchor";
import { Program, Wallet } from "@coral-xyz/anchor";
import { 
  Connection, 
  Keypair, 
  PublicKey, 
  SystemProgram, 
  SYSVAR_RENT_PUBKEY, 
  clusterApiUrl 
} from "@solana/web3.js";
import { 
  TOKEN_PROGRAM_ID, 
  createMint, 
  getOrCreateAssociatedTokenAccount, 
  mintTo 
} from "@solana/spl-token";
import axios from "axios";
import { Resend } from "resend";
import * as crypto from "crypto";

// Environment Configuration
const RESEND_API_KEY = process.env.RESEND_API_KEY || "re_test_mock_123456789";
const NOTIFICATION_WEBHOOK_URL = process.env.DISCORD_SLACK_WEBHOOK || "https://httpbin.org/post";
const RPC_ENDPOINT = process.env.SOLANA_RPC_URL || clusterApiUrl("devnet");

const resend = new Resend(RESEND_API_KEY);

export class CampusFlowClient {
  private connection: Connection;
  private wallet: Wallet;
  private programId: PublicKey;

  constructor(keypair: Keypair, programIdStr: string) {
    this.connection = new Connection(RPC_ENDPOINT, "confirmed");
    this.wallet = new Wallet(keypair);
    this.programId = new PublicKey(programIdStr);
  }

  /**
   * Derives Grant PDA address
   */
  public getGrantPDA(sponsor: PublicKey, grantId: anchor.BN): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), sponsor.toBuffer(), grantId.toArrayLike(Buffer, "le", 8)],
      this.programId
    );
  }

  /**
   * Derives Escrow Vault PDA address
   */
  public getVaultPDA(grantPda: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), grantPda.toBuffer()],
      this.programId
    );
  }

  /**
   * Derives Student Profile PDA address
   */
  public getStudentPDA(student: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("student"), student.toBuffer()],
      this.programId
    );
  }

  /**
   * Derives Milestone PDA address
   */
  public getMilestonePDA(grantPda: PublicKey, studentProfilePda: PublicKey, index: number): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("milestone"), grantPda.toBuffer(), studentProfilePda.toBuffer(), Buffer.from([index])],
      this.programId
    );
  }

  /**
   * Dispatches Enterprise Alerts via Webhook and Resend Email
   */
  public async dispatchEnterpriseAlerts(params: {
    event: string;
    studentEmail: string;
    builder: string;
    amount: string;
    txHash: string;
  }) {
    console.log(`[Enterprise Notification Engine] Dispatching alert: ${params.event}`);

    // 1. Dispatch Webhook to Slack / Discord
    try {
      await axios.post(NOTIFICATION_WEBHOOK_URL, {
        content: `🚨 **CampusFlow Protocol Update**: Milestone Approved!`,
        embeds: [{
          title: `Payout Disbursed: ${params.amount} tokens`,
          color: 0x14F195, // Solana Green
          fields: [
            { name: "Recipient", value: `\`${params.builder}\``, inline: true },
            { name: "Tx Signature", value: `\`${params.txHash.slice(0, 16)}...\``, inline: true },
            { name: "Timestamp", value: new Date().toISOString(), inline: false }
          ]
        }]
      });
      console.log("✅ Webhook payload delivered successfully.");
    } catch (err: any) {
      console.warn("⚠️ Webhook delivery skipped or returned error:", err.message);
    }

    // 2. Dispatch Formal Confirmation Email via Resend
    try {
      if (RESEND_API_KEY && !RESEND_API_KEY.includes("mock")) {
        await resend.emails.send({
          from: "grants@campusflow.io",
          to: params.studentEmail,
          subject: "🎉 Grant Milestone Approved - Funds Disbursed",
          html: `
            <h2>Congratulations Builder!</h2>
            <p>Your milestone deliverable has met all proof-of-vibe validation checks.</p>
            <p><strong>Disbursed Amount:</strong> ${params.amount}</p>
            <p><strong>Solana Transaction:</strong> <code>${params.txHash}</code></p>
            <p>Keep building collegiate excellence with CampusFlow!</p>
          `,
        });
        console.log(`✅ Resend email notification queued to: ${params.studentEmail}`);
      }
    } catch (err: any) {
      console.warn("⚠️ Resend API email delivery skipped:", err.message);
    }
  }

  /**
   * Generates deterministic collegiate institution hash from verified academic email
   */
  public static hashAcademicDomain(email: string): Buffer {
    const domain = email.split("@")[1].toLowerCase();
    return crypto.createHash("sha256").update(domain).digest();
  }
}