import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { UnivibeCore } from "../target/types/univibe_core";
import { PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { expect } from "chai";

describe("univibe_core protocol test suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.UnivibeCore as Program<UnivibeCore>;

  const admin = Keypair.generate();
  const treasury = Keypair.generate();
  const creator = Keypair.generate();
  const donor = Keypair.generate();
  const campaignId = new anchor.BN(42);

  let configPda: PublicKey;
  let campaignPda: PublicKey;
  let vaultPda: PublicKey;

  before(async () => {
    // Airdrop SOL to test actors
    const airdropSigners = [admin, creator, donor];
    for (const actor of airdropSigners) {
      const tx = await provider.connection.requestAirdrop(actor.publicKey, 10 * LAMPORTS_PER_SOL);
      const latestBlockhash = await provider.connection.getLatestBlockhash();
      await provider.connection.confirmTransaction({
        signature: tx,
        blockhash: latestBlockhash.blockhash,
        lastValidBlockHeight: latestBlockhash.lastValidBlockHeight,
      });
    }

    [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], program.programId);

    [campaignPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), creator.publicKey.toBuffer(), campaignId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), creator.publicKey.toBuffer(), campaignId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
  });

  it("1. Initializes global protocol configuration with 1% fee (100 bps)", async () => {
    const feeBps = 100; // 1%
    await program.methods
      .initializeProtocol(feeBps)
      .accounts({
        config: configPda,
        admin: admin.publicKey,
        treasury: treasury.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    const configAccount = await program.account.protocolConfig.fetch(configPda);
    expect(configAccount.admin.toBase58()).to.equal(admin.publicKey.toBase58());
    expect(configAccount.treasury.toBase58()).to.equal(treasury.publicKey.toBase58());
    expect(configAccount.feeBasisPoints).to.equal(feeBps);
    expect(configAccount.totalCampaigns.toNumber()).to.equal(0);
  });

  it("2. Rejects campaign creation when milestone percentages do not sum to 100", async () => {
    const invalidMilestones = Buffer.from([50, 40]); // Sum = 90
    try {
      await program.methods
        .createCampaign(
          campaignId,
          "Campus Autonomous Rover",
          "Student robotics lab milestone grant",
          new anchor.BN(5 * LAMPORTS_PER_SOL),
          invalidMilestones
        )
        .accounts({
          campaign: campaignPda,
          vault: vaultPda,
          config: configPda,
          creator: creator.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([creator])
        .rpc();
      expect.fail("Should have failed with MilestonesMustSumTo100");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("MilestonesMustSumTo100");
    }
  });

  it("3. Successfully creates a campaign with 3 milestone gates (40%, 30%, 30%)", async () => {
    const validMilestones = Buffer.from([40, 30, 30]); // Sum = 100
    await program.methods
      .createCampaign(
        campaignId,
        "Campus Autonomous Rover",
        "Student robotics lab milestone grant",
        new anchor.BN(5 * LAMPORTS_PER_SOL),
        validMilestones
      )
      .accounts({
        campaign: campaignPda,
        vault: vaultPda,
        config: configPda,
        creator: creator.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([creator])
      .rpc();

    const campaignAccount = await program.account.campaignAccount.fetch(campaignPda);
    expect(campaignAccount.name).to.equal("Campus Autonomous Rover");
    expect(campaignAccount.targetAmount.toNumber()).to.equal(5 * LAMPORTS_PER_SOL);
    expect(campaignAccount.collectedAmount.toNumber()).to.equal(0);
    expect(campaignAccount.currentMilestone).to.equal(0);
    expect(campaignAccount.isCompleted).to.be.false;

    const configAccount = await program.account.protocolConfig.fetch(configPda);
    expect(configAccount.totalCampaigns.toNumber()).to.equal(1);
  });

  it("4. Accepts donor contributions and accurately updates campaign PDA state", async () => {
    const donationAmount = new anchor.BN(2 * LAMPORTS_PER_SOL);

    const vaultBalanceBefore = await provider.connection.getBalance(vaultPda);

    await program.methods
      .donate(donationAmount)
      .accounts({
        campaign: campaignPda,
        vault: vaultPda,
        donor: donor.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([donor])
      .rpc();

    const vaultBalanceAfter = await provider.connection.getBalance(vaultPda);
    expect(vaultBalanceAfter - vaultBalanceBefore).to.equal(donationAmount.toNumber());

    const campaignAccount = await program.account.campaignAccount.fetch(campaignPda);
    expect(campaignAccount.collectedAmount.toNumber()).to.equal(donationAmount.toNumber());
  });

  it("5. Prevents unauthorized non-creators from releasing milestone funds", async () => {
    const impostor = Keypair.generate();
    await provider.connection.requestAirdrop(impostor.publicKey, LAMPORTS_PER_SOL);

    try {
      await program.methods
        .releaseMilestone()
        .accounts({
          campaign: campaignPda,
          vault: vaultPda,
          config: configPda,
          creator: impostor.publicKey, // Unauthorized!
          treasury: treasury.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([impostor])
        .rpc();
      expect.fail("Should have thrown UnauthorizedCreator error");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("UnauthorizedCreator");
    }
  });

  it("6. Creator successfully releases Milestone 0: verifies net payout & treasury fee deductions", async () => {
    const creatorBalBefore = await provider.connection.getBalance(creator.publicKey);
    const treasuryBalBefore = await provider.connection.getBalance(treasury.publicKey);

    // Milestone 0 is 40% of 2 SOL = 0.8 SOL.
    // Fee is 1% of 0.8 SOL = 0.008 SOL.
    // Net payout to creator = 0.792 SOL.
    await program.methods
      .releaseMilestone()
      .accounts({
        campaign: campaignPda,
        vault: vaultPda,
        config: configPda,
        creator: creator.publicKey,
        treasury: treasury.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([creator])
      .rpc();

    const creatorBalAfter = await provider.connection.getBalance(creator.publicKey);
    const treasuryBalAfter = await provider.connection.getBalance(treasury.publicKey);

    const expectedGross = (2 * LAMPORTS_PER_SOL * 40) / 100;
    const expectedFee = (expectedGross * 100) / 10000;
    const expectedNet = expectedGross - expectedFee;

    expect(treasuryBalAfter - treasuryBalBefore).to.equal(expectedFee);
    // Allow slight tolerance for transaction fee paid by creator
    expect(creatorBalAfter - creatorBalBefore).to.be.closeTo(expectedNet, 5000000);

    const campaignAccount = await program.account.campaignAccount.fetch(campaignPda);
    expect(campaignAccount.currentMilestone).to.equal(1);
    expect(campaignAccount.withdrawnAmount.toNumber()).to.equal(expectedGross);
  });
});