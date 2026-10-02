import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { CampusFlow } from "../target/types/campus_flow";
import { 
  Keypair, 
  PublicKey, 
  SystemProgram, 
  SYSVAR_RENT_PUBKEY 
} from "@solana/web3.js";
import { 
  TOKEN_PROGRAM_ID, 
  createMint, 
  createAccount, 
  mintTo, 
  getAccount 
} from "@solana/spl-token";
import { expect } from "chai";
import * as crypto from "crypto";

describe("CampusFlow End-to-End Test Suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.CampusFlow as Program<CampusFlow>;

  let mint: PublicKey;
  const sponsor = Keypair.generate();
  const builder = Keypair.generate();
  const peer1 = Keypair.generate();
  const peer2 = Keypair.generate();
  const peer3 = Keypair.generate();

  let sponsorAta: PublicKey;
  let builderAta: PublicKey;

  const grantId = new anchor.BN(9001);
  const totalBudget = new anchor.BN(10_000_000_000); // 10,000 Tokens (6 decimals)
  const milestonePayout = new anchor.BN(2_500_000_000); // 2,500 Tokens

  let grantPda: PublicKey;
  let grantBump: number;
  let vaultPda: PublicKey;
  let vaultBump: number;

  let builderProfilePda: PublicKey;
  let milestonePda: PublicKey;

  before(async () => {
    // Airdrop SOL to actors
    const airdropSigners = [sponsor, builder, peer1, peer2, peer3];
    for (const signer of airdropSigners) {
      const sig = await provider.connection.requestAirdrop(signer.publicKey, 5 * anchor.web3.LAMPORTS_PER_SOL);
      const latestBlockhash = await provider.connection.getLatestBlockhash();
      await provider.connection.confirmTransaction({
        signature: sig,
        ...latestBlockhash
      });
    }

    // Create Grant SPL Token Mint
    mint = await createMint(
      provider.connection,
      sponsor,
      sponsor.publicKey,
      null,
      6
    );

    // Create Token Accounts
    sponsorAta = await createAccount(provider.connection, sponsor, mint, sponsor.publicKey);
    builderAta = await createAccount(provider.connection, builder, mint, builder.publicKey);

    // Mint budget to Sponsor ATA
    await mintTo(provider.connection, sponsor, mint, sponsorAta, sponsor, 10_000_000_000);

    // Derive PDAs
    [grantPda, grantBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), sponsor.publicKey.toBuffer(), grantId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    [vaultPda, vaultBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), grantPda.toBuffer()],
      program.programId
    );

    [builderProfilePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("student"), builder.publicKey.toBuffer()],
      program.programId
    );

    [milestonePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("milestone"), grantPda.toBuffer(), builderProfilePda.toBuffer(), Buffer.from([0])],
      program.programId
    );
  });

  it("1. Successfully initializes a collegiate hackathon grant vault", async () => {
    const title = "UNI Vibeathon Grand Prize Escrow";

    await program.methods
      .initializeGrant(grantId, title, totalBudget)
      .accounts({
        sponsor: sponsor.publicKey,
        grant: grantPda,
        mint: mint,
        vault: vaultPda,
        sponsorTokenAccount: sponsorAta,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([sponsor])
      .rpc();

    const grantAccount = await program.account.grant.fetch(grantPda);
    expect(grantAccount.totalBudget.toString()).to.equal(totalBudget.toString());
    expect(grantAccount.disbursedAmount.toString()).to.equal("0");
    expect(grantAccount.title).to.equal(title);

    const vaultAccount = await getAccount(provider.connection, vaultPda);
    expect(vaultAccount.amount.toString()).to.equal(totalBudget.toString());
  });

  it("2. Registers builder with verified academic institution hash", async () => {
    const institutionHash = crypto.createHash("sha256").update("stanford.edu").digest();

    await program.methods
      .registerStudent(Array.from(institutionHash))
      .accounts({
        student: builder.publicKey,
        studentProfile: builderProfilePda,
        systemProgram: SystemProgram.programId,
      })
      .signers([builder])
      .rpc();

    const studentRecord = await program.account.studentProfile.fetch(builderProfilePda);
    expect(studentRecord.reputationScore).to.equal(100);
    expect(studentRecord.vouchCount).to.equal(0);
  });

  it("3. Builder submits project milestone deliverable", async () => {
    const deliverableUri = "https://github.com/vibe-builder/solana-autonomous-agent";

    await program.methods
      .submitMilestone(0, deliverableUri, milestonePayout)
      .accounts({
        student: builder.publicKey,
        studentProfile: builderProfilePda,
        grant: grantPda,
        milestone: milestonePda,
        systemProgram: SystemProgram.programId,
      })
      .signers([builder])
      .rpc();

    const milestoneRecord = await program.account.milestone.fetch(milestonePda);
    expect(milestoneRecord.payoutAmount.toString()).to.equal(milestonePayout.toString());
    expect(milestoneRecord.vouchCount).to.equal(0);
    expect(milestoneRecord.isDisbursed).to.be.false;
  });

  it("4. Prevents unauthorized release before reaching minimum peer vouches", async () => {
    try {
      await program.methods
        .releaseMilestone(grantId)
        .accounts({
          sponsor: sponsor.publicKey,
          builder: builder.publicKey,
          builderProfile: builderProfilePda,
          grant: grantPda,
          milestone: milestonePda,
          vault: vaultPda,
          builderTokenAccount: builderAta,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([sponsor])
        .rpc();
      expect.fail("Release should have failed due to insufficient vouches");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("NotEnoughVouches");
    }
  });

  it("5. Allows collegiate peers to cast Proof-of-Vibe vouches", async () => {
    const peers = [peer1, peer2, peer3];
    const dummyHash = crypto.createHash("sha256").update("mit.edu").digest();

    for (const peer of peers) {
      const [peerProfilePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("student"), peer.publicKey.toBuffer()],
        program.programId
      );

      // Register peer
      await program.methods
        .registerStudent(Array.from(dummyHash))
        .accounts({
          student: peer.publicKey,
          studentProfile: peerProfilePda,
          systemProgram: SystemProgram.programId,
        })
        .signers([peer])
        .rpc();

      // Cast vouch
      await program.methods
        .castVouch()
        .accounts({
          voter: peer.publicKey,
          voterProfile: peerProfilePda,
          builderProfile: builderProfilePda,
          milestone: milestonePda,
        })
        .signers([peer])
        .rpc();
    }

    const milestoneRecord = await program.account.milestone.fetch(milestonePda);
    expect(milestoneRecord.vouchCount).to.equal(3);
  });

  it("6. Rejects self-vouching attempts by builder", async () => {
    try {
      await program.methods
        .castVouch()
        .accounts({
          voter: builder.publicKey,
          voterProfile: builderProfilePda,
          builderProfile: builderProfilePda,
          milestone: milestonePda,
        })
        .signers([builder])
        .rpc();
      expect.fail("Self-vouch should be blocked");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("SelfVouchingNotAllowed");
    }
  });

  it("7. Disburses milestone tokens securely to builder token account", async () => {
    const builderBalanceBefore = (await getAccount(provider.connection, builderAta)).amount;

    await program.methods
      .releaseMilestone(grantId)
      .accounts({
        sponsor: sponsor.publicKey,
        builder: builder.publicKey,
        builderProfile: builderProfilePda,
        grant: grantPda,
        milestone: milestonePda,
        vault: vaultPda,
        builderTokenAccount: builderAta,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([sponsor])
      .rpc();

    const builderBalanceAfter = (await getAccount(provider.connection, builderAta)).amount;
    expect(BigInt(builderBalanceAfter) - BigInt(builderBalanceBefore)).to.equal(BigInt(milestonePayout.toString()));

    const milestoneRecord = await program.account.milestone.fetch(milestonePda);
    expect(milestoneRecord.isDisbursed).to.be.true;
    expect(milestoneRecord.isCompleted).to.be.true;
  });
});