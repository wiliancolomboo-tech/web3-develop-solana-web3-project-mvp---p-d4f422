import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { expect } from "chai";
import { Vibestream } from "../target/types/vibestream";

describe("vibestream-e2e-suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Vibestream as Program<Vibestream>;

  const sponsor = anchor.web3.Keypair.generate();
  const builder = anchor.web3.Keypair.generate();
  const reviewer = anchor.web3.Keypair.generate();
  const adversary = anchor.web3.Keypair.generate();

  let mint: anchor.web3.PublicKey;
  let sponsorTokenAccount: anchor.web3.PublicKey;
  let builderTokenAccount: anchor.web3.PublicKey;

  const grantId = new anchor.BN(42);
  const milestone1 = new anchor.BN(300_000_000); // 300 tokens
  const milestone2 = new anchor.BN(700_000_000); // 700 tokens
  const totalFunding = milestone1.add(milestone2);

  let grantPda: anchor.web3.PublicKey;
  let vaultPda: anchor.web3.PublicKey;

  before(async () => {
    // Fund sponsor and builder with Native SOL for rent/tx fees
    for (const actor of [sponsor, builder, reviewer, adversary]) {
      const sig = await provider.connection.requestAirdrop(
        actor.publicKey,
        5 * anchor.web3.LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig, "confirmed");
    }

    // Create Token Mint
    mint = await createMint(
      provider.connection,
      sponsor,
      sponsor.publicKey,
      null,
      6
    );

    // Create Token Accounts
    sponsorTokenAccount = await createAccount(
      provider.connection,
      sponsor,
      mint,
      sponsor.publicKey
    );

    builderTokenAccount = await createAccount(
      provider.connection,
      builder,
      mint,
      builder.publicKey
    );

    // Mint supply to sponsor
    await mintTo(
      provider.connection,
      sponsor,
      mint,
      sponsorTokenAccount,
      sponsor,
      2_000_000_000 // 2,000 tokens
    );

    // Derive PDAs
    const grantIdBuf = grantId.toArrayLike(Buffer, "le", 8);

    [grantPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), sponsor.publicKey.toBuffer(), grantIdBuf],
      program.programId
    );

    [vaultPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), sponsor.publicKey.toBuffer(), grantIdBuf],
      program.programId
    );
  });

  it("1. Initializes a milestone grant and locks tokens into Escrow PDA", async () => {
    await program.methods
      .initializeGrant(
        grantId,
        "UNI Vibeathon Hackathon Track Winner",
        [milestone1, milestone2],
        reviewer.publicKey
      )
      .accounts({
        grantState: grantPda,
        vault: vaultPda,
        mint: mint,
        sponsorToken: sponsorTokenAccount,
        recipient: builder.publicKey,
        sponsor: sponsor.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([sponsor])
      .rpc();

    const vaultInfo = await getAccount(provider.connection, vaultPda);
    expect(Number(vaultInfo.amount)).to.equal(totalFunding.toNumber());

    const grantState = await program.account.grantState.fetch(grantPda);
    expect(grantState.sponsor.toBase58()).to.equal(sponsor.publicKey.toBase58());
    expect(grantState.recipient.toBase58()).to.equal(builder.publicKey.toBase58());
    expect(grantState.currentMilestoneIndex).to.equal(0);
    expect(grantState.isActive).to.be.true;
    expect(grantState.milestones.length).to.equal(2);
  });

  it("2. Adversary fails to submit milestone (Unauthorized recipient)", async () => {
    try {
      await program.methods
        .submitMilestone(grantId, 0, "https://github.com/adversary/fake-submission")
        .accounts({
          grantState: grantPda,
          recipient: adversary.publicKey,
        })
        .signers([adversary])
        .rpc();
      expect.fail("Should have thrown unauthorized error");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("UnauthorizedRecipient");
    }
  });

  it("3. Builder submits milestone deliverable URI", async () => {
    const proofUri = "https://github.com/vibestream/student-project/commit/abcdef123";
    await program.methods
      .submitMilestone(grantId, 0, proofUri)
      .accounts({
        grantState: grantPda,
        recipient: builder.publicKey,
      })
      .signers([builder])
      .rpc();

    const grantState = await program.account.grantState.fetch(grantPda);
    const ms0 = grantState.milestones[0];
    expect(ms0.status).to.deep.equal({ submitted: {} });
  });

  it("4. Reviewer signs off and approves Milestone 0", async () => {
    await program.methods
      .reviewMilestone(grantId, 0, true)
      .accounts({
        grantState: grantPda,
        reviewer: reviewer.publicKey,
      })
      .signers([reviewer])
      .rpc();

    const grantState = await program.account.grantState.fetch(grantPda);
    expect(grantState.milestones[0].status).to.deep.equal({ approved: {} });
    expect(grantState.currentMilestoneIndex).to.equal(1);
  });

  it("5. Builder claims unlocked Milestone 0 funds", async () => {
    const beforeBal = await getAccount(provider.connection, builderTokenAccount);

    await program.methods
      .claimFunds(grantId)
      .accounts({
        grantState: grantPda,
        vault: vaultPda,
        recipientToken: builderTokenAccount,
        recipient: builder.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([builder])
      .rpc();

    const afterBal = await getAccount(provider.connection, builderTokenAccount);
    const claimed = BigInt(afterBal.amount) - BigInt(beforeBal.amount);
    expect(claimed.toString()).to.equal(milestone1.toString());

    const grantState = await program.account.grantState.fetch(grantPda);
    expect(grantState.claimedAmount.toNumber()).to.equal(milestone1.toNumber());
  });

  it("6. Builder cannot double-claim funds when none are pending", async () => {
    try {
      await program.methods
        .claimFunds(grantId)
        .accounts({
          grantState: grantPda,
          vault: vaultPda,
          recipientToken: builderTokenAccount,
          recipient: builder.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([builder])
        .rpc();
      expect.fail("Double-claim must fail");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("NoFundsAvailable");
    }
  });

  it("7. Sponsor executes clawback on remaining milestone after builder default", async () => {
    const beforeSponsorBal = await getAccount(provider.connection, sponsorTokenAccount);

    await program.methods
      .cancelAndClawback(grantId)
      .accounts({
        grantState: grantPda,
        vault: vaultPda,
        sponsorToken: sponsorTokenAccount,
        sponsor: sponsor.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([sponsor])
      .rpc();

    const afterSponsorBal = await getAccount(provider.connection, sponsorTokenAccount);
    const reclaimed = BigInt(afterSponsorBal.amount) - BigInt(beforeSponsorBal.amount);
    expect(reclaimed.toString()).to.equal(milestone2.toString());

    const grantState = await program.account.grantState.fetch(grantPda);
    expect(grantState.isActive).to.be.false;

    const vaultInfo = await getAccount(provider.connection, vaultPda);
    expect(Number(vaultInfo.amount)).to.equal(0);
  });
});