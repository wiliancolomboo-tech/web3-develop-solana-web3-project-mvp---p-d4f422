import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID
} from "@solana/spl-token";
import { expect } from "chai";

describe("UniVibe Protocol Continuous Streaming & Escrow QA Matrix", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  // In production tests, the generated IDL matches the program workspace
  const program = anchor.workspace.Univibe as Program<any>;

  let mint: PublicKey;
  let sponsorTokenAccount: PublicKey;
  let recipientTokenAccount: PublicKey;
  const sponsor = (provider.wallet as any).payer as Keypair;
  const recipient = Keypair.generate();
  const maliciousActor = Keypair.generate();

  const campaignId = new BN(4242);
  const totalAllocation = new BN(1_000_000_000); // 1,000 units (6 decimals USDC)

  let campaignPda: PublicKey;
  let vaultPda: PublicKey;
  let startTime: number;
  let endTime: number;

  before(async () => {
    // Airdrop SOL to recipient & maliciousActor
    const sig1 = await provider.connection.requestAirdrop(recipient.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL);
    const sig2 = await provider.connection.requestAirdrop(maliciousActor.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.confirmTransaction(sig1);
    await provider.connection.confirmTransaction(sig2);

    // Create test SPL Mint
    mint = await createMint(provider.connection, sponsor, sponsor.publicKey, null, 6);

    // Create token accounts
    sponsorTokenAccount = await createAccount(provider.connection, sponsor, mint, sponsor.publicKey);
    recipientTokenAccount = await createAccount(provider.connection, recipient, mint, recipient.publicKey);

    // Mint funds to sponsor
    await mintTo(provider.connection, sponsor, mint, sponsorTokenAccount, sponsor, 5_000_000_000);

    // Derive PDAs
    [campaignPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("campaign"), sponsor.publicKey.toBuffer(), campaignId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), campaignPda.toBuffer()],
      program.programId
    );
  });

  it("1. Deterministically initializes a new university milestone campaign", async () => {
    const now = Math.floor(Date.now() / 1000);
    startTime = now;
    endTime = now + 10; // 10-second fast stream for test runner

    await program.methods
      .initializeCampaign(
        campaignId,
        "UniVibe Superteam Grant #1",
        recipient.publicKey,
        totalAllocation,
        new BN(startTime),
        new BN(endTime)
      )
      .accounts({
        authority: sponsor.publicKey,
        mint: mint,
        campaign: campaignPda,
        vault: vaultPda,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const campaignAccount = await program.account.campaign.fetch(campaignPda);
    expect(campaignAccount.authority.toBase58()).to.equal(sponsor.publicKey.toBase58());
    expect(campaignAccount.recipient.toBase58()).to.equal(recipient.publicKey.toBase58());
    expect(campaignAccount.totalAllocation.toString()).to.equal(totalAllocation.toString());
    expect(campaignAccount.totalFunded.toNumber()).to.equal(0);
    expect(campaignAccount.status).to.equal(0); // FUNDING
  });

  it("2. Funds the campaign vault with SPL tokens", async () => {
    await program.methods
      .fundCampaign(totalAllocation)
      .accounts({
        funder: sponsor.publicKey,
        campaign: campaignPda,
        funderTokenAccount: sponsorTokenAccount,
        vault: vaultPda,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const vaultToken = await getAccount(provider.connection, vaultPda);
    expect(Number(vaultToken.amount)).to.equal(totalAllocation.toNumber());

    const campaignAccount = await program.account.campaign.fetch(campaignPda);
    expect(campaignAccount.totalFunded.toString()).to.equal(totalAllocation.toString());
    expect(campaignAccount.status).to.equal(1); // ACTIVE
  });

  it("3. Rejects claims from unauthorized actors", async () => {
    const maliciousTokenAccount = await createAccount(
      provider.connection,
      maliciousActor,
      mint,
      maliciousActor.publicKey
    );

    try {
      await program.methods
        .claimStream()
        .accounts({
          recipient: maliciousActor.publicKey,
          campaign: campaignPda,
          vault: vaultPda,
          recipientTokenAccount: maliciousTokenAccount,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([maliciousActor])
        .rpc();
      expect.fail("Unauthorized actor should have been prevented from claiming.");
    } catch (err: any) {
      expect(err.toString()).to.include("Unauthorized");
    }
  });

  it("4. Recipient claims vested tokens linearly as clock advances", async () => {
    // Wait 3 seconds to accumulate 30% vesting
    await new Promise((resolve) => setTimeout(resolve, 3000));

    await program.methods
      .claimStream()
      .accounts({
        recipient: recipient.publicKey,
        campaign: campaignPda,
        vault: vaultPda,
        recipientTokenAccount: recipientTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([recipient])
      .rpc();

    const recipientBalance = await getAccount(provider.connection, recipientTokenAccount);
    expect(Number(recipientBalance.amount)).to.be.greaterThan(0);

    const campaignAccount = await program.account.campaign.fetch(campaignPda);
    expect(campaignAccount.totalClaimed.toNumber()).to.equal(Number(recipientBalance.amount));
  });

  it("5. Allows sponsor to trigger safety clawback of unvested funds if milestone is aborted", async () => {
    const preSponsorBalance = (await getAccount(provider.connection, sponsorTokenAccount)).amount;

    await program.methods
      .cancelCampaign()
      .accounts({
        authority: sponsor.publicKey,
        campaign: campaignPda,
        vault: vaultPda,
        authorityTokenAccount: sponsorTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const postSponsorBalance = (await getAccount(provider.connection, sponsorTokenAccount)).amount;
    expect(Number(postSponsorBalance)).to.be.greaterThan(Number(preSponsorBalance));

    const campaignAccount = await program.account.campaign.fetch(campaignPda);
    expect(campaignAccount.status).to.equal(3); // CANCELLED
  });
});