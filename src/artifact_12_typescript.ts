import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { UnivibeCore } from "../target/types/univibe_core";
import { PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { expect } from "chai";
import * as crypto from "crypto";

describe("univibe_core test suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.UnivibeCore as Program<UnivibeCore>;
  const sponsor = Keypair.generate();
  const grantee = Keypair.generate();
  const unauthorizedUser = Keypair.generate();

  const poolId = "vibeathon-track-1";
  let poolPda: PublicKey;
  let vaultPda: PublicKey;

  before(async () => {
    // Airdrop 10 SOL to sponsor and unauthorizedUser
    const airdropSponsor = await provider.connection.requestAirdrop(sponsor.publicKey, 10 * LAMPORTS_PER_SOL);
    const airdropHacker = await provider.connection.requestAirdrop(unauthorizedUser.publicKey, 2 * LAMPORTS_PER_SOL);
    await provider.connection.confirmTransaction(airdropSponsor);
    await provider.connection.confirmTransaction(airdropHacker);

    [poolPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant_pool"), sponsor.publicKey.toBuffer(), Buffer.from(poolId)],
      program.programId
    );

    [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), poolPda.toBuffer()],
      program.programId
    );
  });

  it("1. Successfully initializes a grant pool and escrows 2 SOL in PDA vault", async () => {
    const totalAmount = new anchor.BN(2 * LAMPORTS_PER_SOL);
    const now = Math.floor(Date.now() / 1000);
    const startTime = new anchor.BN(now - 10);
    const endTime = new anchor.BN(now + 100);
    const milestoneCount = 2;

    await program.methods
      .initializeGrantPool(poolId, totalAmount, startTime, endTime, milestoneCount)
      .accounts({
        grantPool: poolPda,
        vault: vaultPda,
        sponsor: sponsor.publicKey,
        grantee: grantee.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    const poolAccount = await program.account.grantPool.fetch(poolPda);
    expect(poolAccount.sponsor.toBase58()).to.equal(sponsor.publicKey.toBase58());
    expect(poolAccount.grantee.toBase58()).to.equal(grantee.publicKey.toBase58());
    expect(poolAccount.totalAmount.toNumber()).to.equal(2 * LAMPORTS_PER_SOL);
    expect(poolAccount.milestoneCount).to.equal(2);
    expect(poolAccount.milestonesApproved).to.equal(0);

    const vaultBalance = await provider.connection.getBalance(vaultPda);
    expect(vaultBalance).to.equal(2 * LAMPORTS_PER_SOL);
  });

  it("2. Fails when out-of-order milestone is submitted", async () => {
    const randomHash = Array.from(crypto.randomBytes(32));
    try {
      // Trying to approve milestone 2 before milestone 1
      await program.methods
        .approveMilestone(2, randomHash)
        .accounts({
          grantPool: poolPda,
          sponsor: sponsor.publicKey,
        })
        .signers([sponsor])
        .rpc();
      expect.fail("Should have thrown OutofOrderMilestone error");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("OutofOrderMilestone");
    }
  });

  it("3. Approves sequential Milestone 1 from verified sponsor", async () => {
    const randomHash = Array.from(crypto.randomBytes(32));
    await program.methods
      .approveMilestone(1, randomHash)
      .accounts({
        grantPool: poolPda,
        sponsor: sponsor.publicKey,
      })
      .signers([sponsor])
      .rpc();

    const poolAccount = await program.account.grantPool.fetch(poolPda);
    expect(poolAccount.milestonesApproved).to.equal(1);
  });

  it("4. Prevents unauthorized hacker from claiming funds on behalf of grantee", async () => {
    try {
      await program.methods
        .claimFunds()
        .accounts({
          grantPool: poolPda,
          vault: vaultPda,
          grantee: unauthorizedUser.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([unauthorizedUser])
        .rpc();
      expect.fail("Should fail with UnauthorizedGrantee");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("UnauthorizedGrantee");
    }
  });

  it("5. Allows grantee to claim vested funds with accurate balance check", async () => {
    const balanceBefore = await provider.connection.getBalance(grantee.publicKey);

    await program.methods
      .claimFunds()
      .accounts({
        grantPool: poolPda,
        vault: vaultPda,
        grantee: grantee.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([grantee])
      .rpc();

    const balanceAfter = await provider.connection.getBalance(grantee.publicKey);
    expect(balanceAfter).to.be.greaterThan(balanceBefore);

    const poolAccount = await program.account.grantPool.fetch(poolPda);
    expect(poolAccount.claimedAmount.toNumber()).to.be.greaterThan(0);
  });

  it("6. Sponsor can cancel pool and recover unvested balance", async () => {
    const sponsorBalanceBefore = await provider.connection.getBalance(sponsor.publicKey);

    await program.methods
      .cancelPool()
      .accounts({
        grantPool: poolPda,
        vault: vaultPda,
        sponsor: sponsor.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    const poolAccount = await program.account.grantPool.fetch(poolPda);
    expect(poolAccount.isCancelled).to.be.true;

    const sponsorBalanceAfter = await provider.connection.getBalance(sponsor.publicKey);
    expect(sponsorBalanceAfter).to.be.greaterThan(sponsorBalanceBefore - 0.05 * LAMPORTS_PER_SOL);
  });
});