import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { UnivibeProtocol } from "../target/types/univibe_protocol";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";

describe("univibe_protocol test suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.UnivibeProtocol as Program<UnivibeProtocol>;

  const sponsor = Keypair.generate();
  const beneficiary = Keypair.generate();
  const reviewer = Keypair.generate();
  const unauthorizedUser = Keypair.generate();

  const grantTitle = "AI Agent Hackathon Grant";
  const milestone1Amount = new anchor.BN(0.5 * LAMPORTS_PER_SOL);
  const milestone2Amount = new anchor.BN(0.5 * LAMPORTS_PER_SOL);
  const totalFunding = milestone1Amount.add(milestone2Amount);

  let grantPda: PublicKey;
  let grantBump: number;
  let escrowPda: PublicKey;
  let escrowBump: number;

  before(async () => {
    // Airdrop SOL to test actors
    const airdropSigners = [sponsor, beneficiary, reviewer, unauthorizedUser];
    for (const actor of airdropSigners) {
      const sig = await provider.connection.requestAirdrop(actor.publicKey, 5 * LAMPORTS_PER_SOL);
      const latestBlockhash = await provider.connection.getLatestBlockhash();
      await provider.connection.confirmTransaction({
        signature: sig,
        ...latestBlockhash,
      });
    }

    [grantPda, grantBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), sponsor.publicKey.toBuffer(), Buffer.from(grantTitle)],
      program.programId
    );

    [escrowPda, escrowBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("escrow"), grantPda.toBuffer()],
      program.programId
    );
  });

  it("1. Successfully initializes a grant with 2 milestone escrows", async () => {
    const tx = await program.methods
      .initializeGrant(grantTitle, [milestone1Amount, milestone2Amount])
      .accounts({
        grant: grantPda,
        escrowVault: escrowPda,
        beneficiary: beneficiary.publicKey,
        reviewer: reviewer.publicKey,
        sponsor: sponsor.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    expect(tx).to.be.a("string");

    const grantAccount = await program.account.grant.fetch(grantPda);
    expect(grantAccount.sponsor.toBase58()).to.equal(sponsor.publicKey.toBase58());
    expect(grantAccount.beneficiary.toBase58()).to.equal(beneficiary.publicKey.toBase58());
    expect(grantAccount.reviewer.toBase58()).to.equal(reviewer.publicKey.toBase58());
    expect(grantAccount.totalAmount.toNumber()).to.equal(totalFunding.toNumber());
    expect(grantAccount.claimedAmount.toNumber()).to.equal(0);
    expect(grantAccount.milestoneCount).to.equal(2);
    expect(grantAccount.isActive).to.be.true;

    // Verify escrow balance
    const escrowBalance = await provider.connection.getBalance(escrowPda);
    expect(escrowBalance).to.equal(totalFunding.toNumber());
  });

  it("2. Beneficiary submits deliverable for Milestone #0", async () => {
    const deliverableUri = "https://github.com/univibe-campus/hackathon-agent-mvp";

    await program.methods
      .submitMilestone(0, deliverableUri)
      .accounts({
        grant: grantPda,
        beneficiary: beneficiary.publicKey,
      })
      .signers([beneficiary])
      .rpc();

    const grantAccount = await program.account.grant.fetch(grantPda);
    const m0 = grantAccount.milestones[0];
    expect(m0.status).to.deep.equal({ underReview: {} });
    expect(m0.submittedAt.toNumber()).to.be.greaterThan(0);
  });

  it("3. Rejects approval from non-reviewer unauthorized user", async () => {
    try {
      await program.methods
        .approveMilestone(0)
        .accounts({
          grant: grantPda,
          reviewer: unauthorizedUser.publicKey,
        })
        .signers([unauthorizedUser])
        .rpc();
      expect.fail("Should have failed with unauthorized reviewer error");
    } catch (err: any) {
      expect(err.toString()).to.include("Unauthorized");
    }
  });

  it("4. Authorized reviewer successfully approves Milestone #0", async () => {
    await program.methods
      .approveMilestone(0)
      .accounts({
        grant: grantPda,
        reviewer: reviewer.publicKey,
      })
      .signers([reviewer])
      .rpc();

    const grantAccount = await program.account.grant.fetch(grantPda);
    const m0 = grantAccount.milestones[0];
    expect(m0.status).to.deep.equal({ approved: {} });
    expect(m0.approvedAt.toNumber()).to.be.greaterThan(0);
  });

  it("5. Releases funds for Milestone #0 directly to beneficiary", async () => {
    const initialBeneficiaryBalance = await provider.connection.getBalance(beneficiary.publicKey);

    await program.methods
      .releaseFunds(0)
      .accounts({
        grant: grantPda,
        escrowVault: escrowPda,
        beneficiary: beneficiary.publicKey,
        caller: sponsor.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    const postBeneficiaryBalance = await provider.connection.getBalance(beneficiary.publicKey);
    expect(postBeneficiaryBalance - initialBeneficiaryBalance).to.equal(milestone1Amount.toNumber());

    const grantAccount = await program.account.grant.fetch(grantPda);
    expect(grantAccount.claimedAmount.toNumber()).to.equal(milestone1Amount.toNumber());
    expect(grantAccount.milestones[0].status).to.deep.equal({ claimed: {} });
    expect(grantAccount.isActive).to.be.true; // Second milestone remains
  });

  it("6. Rejects double release attempt on already claimed milestone", async () => {
    try {
      await program.methods
        .releaseFunds(0)
        .accounts({
          grant: grantPda,
          escrowVault: escrowPda,
          beneficiary: beneficiary.publicKey,
          caller: sponsor.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([sponsor])
        .rpc();
      expect.fail("Double claiming must fail");
    } catch (err: any) {
      expect(err.toString()).to.include("MilestoneNotApproved");
    }
  });

  it("7. Completes Milestone #1 and automatically closes the grant", async () => {
    // Submit
    await program.methods
      .submitMilestone(1, "https://demo.univibe.xyz/final-demo-recording")
      .accounts({ grant: grantPda, beneficiary: beneficiary.publicKey })
      .signers([beneficiary])
      .rpc();

    // Approve
    await program.methods
      .approveMilestone(1)
      .accounts({ grant: grantPda, reviewer: reviewer.publicKey })
      .signers([reviewer])
      .rpc();

    // Release
    await program.methods
      .releaseFunds(1)
      .accounts({
        grant: grantPda,
        escrowVault: escrowPda,
        beneficiary: beneficiary.publicKey,
        caller: beneficiary.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([beneficiary])
      .rpc();

    const finalGrant = await program.account.grant.fetch(grantPda);
    expect(finalGrant.claimedAmount.toNumber()).to.equal(totalFunding.toNumber());
    expect(finalGrant.isActive).to.be.false; // Completed & finalized
    expect(finalGrant.milestones[1].status).to.deep.equal({ claimed: {} });

    // Escrow PDA balance should now be 0
    const remainingEscrowBalance = await provider.connection.getBalance(escrowPda);
    expect(remainingEscrowBalance).to.equal(0);
  });
});