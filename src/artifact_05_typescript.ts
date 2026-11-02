import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";
import * as crypto from "crypto";

describe("campus_vibe protocol test suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  // We read the IDL compiled by Anchor
  const program = anchor.workspace.CampusVibe as Program<any>;

  const grantor = Keypair.generate();
  const recipient = Keypair.generate();
  const reviewer = Keypair.generate();

  const grantId = new anchor.BN(9942);
  const milestoneAmounts = [
    new anchor.BN(0.5 * LAMPORTS_PER_SOL),
    new anchor.BN(0.5 * LAMPORTS_PER_SOL),
  ];
  const totalBudget = 1.0 * LAMPORTS_PER_SOL;

  let grantPDA: PublicKey;
  let vaultPDA: PublicKey;
  let grantBump: number;
  let vaultBump: number;

  before(async () => {
    // Airdrop funds to Grantor and Reviewer
    const sig1 = await provider.connection.requestAirdrop(
      grantor.publicKey,
      5 * LAMPORTS_PER_SOL
    );
    const sig2 = await provider.connection.requestAirdrop(
      reviewer.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(sig1);
    await provider.connection.confirmTransaction(sig2);

    [grantPDA, grantBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), grantor.publicKey.toBuffer(), grantId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    [vaultPDA, vaultBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), grantPDA.toBuffer()],
      program.programId
    );
  });

  it("1. Successfully initializes a grant escrow and locks SOL into Vault PDA", async () => {
    const title = "Autonomous Drone Fleet Research";

    await program.methods
      .initializeGrant(grantId, title, milestoneAmounts, reviewer.publicKey)
      .accounts({
        grantAccount: grantPDA,
        vaultAccount: vaultPDA,
        grantor: grantor.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([grantor])
      .rpc();

    const grantAccount = await program.account.grantAccount.fetch(grantPDA);
    expect(grantAccount.grantId.toNumber()).to.equal(grantId.toNumber());
    expect(grantAccount.totalBudget.toNumber()).to.equal(totalBudget);
    expect(grantAccount.remainingBalance.toNumber()).to.equal(totalBudget);
    expect(grantAccount.currentMilestoneIndex).to.equal(0);
    expect(grantAccount.isActive).to.be.true;

    const vaultBalance = await provider.connection.getBalance(vaultPDA);
    expect(vaultBalance).to.equal(totalBudget);
  });

  it("2. Submits proof for milestone 0 by the recipient", async () => {
    const proofHash = crypto.randomBytes(32);

    await program.methods
      .submitMilestone(grantId, Array.from(proofHash))
      .accounts({
        grantAccount: grantPDA,
        recipient: recipient.publicKey,
      })
      .signers([recipient])
      .rpc();

    const grantAccount = await program.account.grantAccount.fetch(grantPDA);
    expect(grantAccount.milestones[0].status).to.deep.equal({ underReview: {} });
  });

  it("3. Rejects milestone approval if non-authorized party attempts verification", async () => {
    const imposter = Keypair.generate();
    const imposterAirdrop = await provider.connection.requestAirdrop(
      imposter.publicKey,
      1 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(imposterAirdrop);

    try {
      await program.methods
        .verifyAndDisburse(grantId, 0)
        .accounts({
          grantAccount: grantPDA,
          vaultAccount: vaultPDA,
          reviewer: imposter.publicKey,
          recipient: recipient.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([imposter])
        .rpc();
      expect.fail("Expected transaction to fail due to UnauthorizedReviewer");
    } catch (err: any) {
      expect(err.toString()).to.include("UnauthorizedReviewer");
    }
  });

  it("4. Reviewer signs off and executes disbursement of milestone 0", async () => {
    const recipientBalanceBefore = await provider.connection.getBalance(recipient.publicKey);

    await program.methods
      .verifyAndDisburse(grantId, 0)
      .accounts({
        grantAccount: grantPDA,
        vaultAccount: vaultPDA,
        reviewer: reviewer.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([reviewer])
      .rpc();

    const recipientBalanceAfter = await provider.connection.getBalance(recipient.publicKey);
    expect(recipientBalanceAfter - recipientBalanceBefore).to.equal(0.5 * LAMPORTS_PER_SOL);

    const grantAccount = await program.account.grantAccount.fetch(grantPDA);
    expect(grantAccount.currentMilestoneIndex).to.equal(1);
    expect(grantAccount.remainingBalance.toNumber()).to.equal(0.5 * LAMPORTS_PER_SOL);
  });

  it("5. Grantor can cancel inactive/abandoned remaining milestones and receive refund", async () => {
    const grantorBalanceBefore = await provider.connection.getBalance(grantor.publicKey);

    await program.methods
      .cancelAndRefund(grantId)
      .accounts({
        grantAccount: grantPDA,
        vaultAccount: vaultPDA,
        grantor: grantor.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([grantor])
      .rpc();

    const grantAccount = await program.account.grantAccount.fetch(grantPDA);
    expect(grantAccount.isActive).to.be.false;
    expect(grantAccount.remainingBalance.toNumber()).to.equal(0);

    const vaultBalanceAfter = await provider.connection.getBalance(vaultPDA);
    expect(vaultBalanceAfter).to.equal(0);
  });
});