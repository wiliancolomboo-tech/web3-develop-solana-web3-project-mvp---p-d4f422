use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("V1bE5tReAm111111111111111111111111111111111");

pub const MAX_METADATA_LEN: usize = 96; // Arweave / IPFS Hash or URI
pub const MAX_MILESTONES: u8 = 10;

#[program]
pub mod vibe_stream {
    use super::*;

    /// Initializes a new milestone stream escrow and deposits the full funding amount into the vault PDA.
    pub fn initialize_stream(
        ctx: Context<InitializeStream>,
        stream_id: u64,
        total_amount: u64,
        total_milestones: u8,
        timelock_deadline: i64,
        metadata_uri: String,
    ) -> Result<()> {
        require!(total_amount > 0, VibeStreamError::ZeroFundingAmount);
        require!(
            total_milestones > 0 && total_milestones <= MAX_MILESTONES,
            VibeStreamError::InvalidMilestoneCount
        );
        require!(
            metadata_uri.as_bytes().len() <= MAX_METADATA_LEN,
            VibeStreamError::MetadataUriTooLong
        );

        let clock = Clock::get()?;
        require!(
            timelock_deadline > clock.unix_timestamp,
            VibeStreamError::InvalidTimelockDeadline
        );

        let stream = &mut ctx.accounts.stream_account;
        stream.funder = ctx.accounts.funder.key();
        stream.beneficiary = ctx.accounts.beneficiary.key();
        stream.evaluator = ctx.accounts.evaluator.key();
        stream.stream_id = stream_id;
        stream.total_amount = total_amount;
        stream.remaining_balance = total_amount;
        stream.total_milestones = total_milestones;
        stream.approved_milestones = 0;
        stream.current_milestone_submitted = false;
        stream.timelock_deadline = timelock_deadline;
        stream.bump = ctx.bumps.stream_account;
        stream.vault_bump = ctx.bumps.vault;
        stream.is_active = true;
        stream.is_cancelled = false;

        // Copy string into fixed slice
        let uri_bytes = metadata_uri.as_bytes();
        let mut fixed_uri = [0u8; MAX_METADATA_LEN];
        fixed_uri[..uri_bytes.len()].copy_from_slice(uri_bytes);
        stream.metadata_uri = fixed_uri;
        stream.metadata_len = uri_bytes.len() as u8;

        // Transfer funds from funder to vault PDA
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.funder.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        );
        transfer(cpi_context, total_amount)?;

        emit!(StreamCreatedEvent {
            stream_pda: stream.key(),
            stream_id,
            funder: stream.funder,
            beneficiary: stream.beneficiary,
            evaluator: stream.evaluator,
            total_amount,
            total_milestones,
            timelock_deadline,
        });

        Ok(())
    }

    /// Grantees submit progress proof (IPFS hash) for the current active milestone.
    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        proof_uri: String,
    ) -> Result<()> {
        let stream = &mut ctx.accounts.stream_account;
        require!(stream.is_active, VibeStreamError::StreamInactive);
        require!(!stream.is_cancelled, VibeStreamError::StreamCancelled);
        require!(
            milestone_index == stream.approved_milestones,
            VibeStreamError::SequentialMilestoneViolation
        );
        require!(
            !stream.current_milestone_submitted,
            VibeStreamError::MilestoneAlreadySubmitted
        );
        require!(
            proof_uri.as_bytes().len() <= MAX_METADATA_LEN,
            VibeStreamError::MetadataUriTooLong
        );

        stream.current_milestone_submitted = true;

        emit!(MilestoneSubmittedEvent {
            stream_pda: stream.key(),
            stream_id: stream.stream_id,
            milestone_index,
            proof_uri,
            beneficiary: stream.beneficiary,
        });

        Ok(())
    }

    /// Evaluator / Hackathon Judge reviews deliverables and approves release of proportional funds.
    pub fn approve_milestone(
        ctx: Context<ApproveMilestone>,
        milestone_index: u8,
    ) -> Result<()> {
        let stream = &mut ctx.accounts.stream_account;
        require!(stream.is_active, VibeStreamError::StreamInactive);
        require!(!stream.is_cancelled, VibeStreamError::StreamCancelled);
        require!(
            stream.current_milestone_submitted,
            VibeStreamError::MilestoneNotSubmitted
        );
        require!(
            milestone_index == stream.approved_milestones,
            VibeStreamError::SequentialMilestoneViolation
        );

        // Compute pro-rata disbursement with full dust clearance on final milestone
        let is_last_milestone = milestone_index == stream.total_milestones - 1;
        let release_amount = if is_last_milestone {
            stream.remaining_balance
        } else {
            stream.total_amount / (stream.total_milestones as u64)
        };

        require!(
            release_amount <= stream.remaining_balance,
            VibeStreamError::ArithmeticOverflow
        );

        // Update state before transfer to guard against reentrancy
        stream.remaining_balance = stream.remaining_balance.saturating_sub(release_amount);
        stream.approved_milestones += 1;
        stream.current_milestone_submitted = false;

        if stream.approved_milestones == stream.total_milestones {
            stream.is_active = false;
        }

        // Perform CPI from Vault PDA to Beneficiary using seeds
        let stream_key = stream.key();
        let vault_seeds: &[&[&[u8]]] = &[&[
            b"vault",
            stream_key.as_ref(),
            &[stream.vault_bump],
        ]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.beneficiary.to_account_info(),
            },
            vault_seeds,
        );
        transfer(cpi_ctx, release_amount)?;

        emit!(MilestoneApprovedEvent {
            stream_pda: stream.key(),
            stream_id: stream.stream_id,
            milestone_index,
            amount_unlocked: release_amount,
            remaining_balance: stream.remaining_balance,
            beneficiary: stream.beneficiary,
            is_completed: !stream.is_active,
        });

        Ok(())
    }

    /// Funder can reclaim remaining vault balance if timelock has elapsed and project is stalled.
    pub fn emergency_clawback(ctx: Context<EmergencyClawback>) -> Result<()> {
        let stream = &mut ctx.accounts.stream_account;
        let clock = Clock::get()?;

        require!(stream.is_active, VibeStreamError::StreamInactive);
        require!(!stream.is_cancelled, VibeStreamError::StreamCancelled);
        require!(
            clock.unix_timestamp >= stream.timelock_deadline,
            VibeStreamError::TimelockNotExpired
        );

        let reclaim_amount = stream.remaining_balance;
        require!(reclaim_amount > 0, VibeStreamError::ZeroFundingAmount);

        stream.remaining_balance = 0;
        stream.is_active = false;
        stream.is_cancelled = true;

        let stream_key = stream.key();
        let vault_seeds: &[&[&[u8]]] = &[&[
            b"vault",
            stream_key.as_ref(),
            &[stream.vault_bump],
        ]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.funder.to_account_info(),
            },
            vault_seeds,
        );
        transfer(cpi_ctx, reclaim_amount)?;

        emit!(StreamClawedBackEvent {
            stream_pda: stream.key(),
            stream_id: stream.stream_id,
            funder: stream.funder,
            amount_reclaimed: reclaim_amount,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION CONTEXTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(stream_id: u64)]
pub struct InitializeStream<'info> {
    #[account(
        init,
        payer = funder,
        space = StreamAccount::LEN,
        seeds = [b"stream", funder.key().as_ref(), &stream_id.to_le_bytes()],
        bump
    )]
    pub stream_account: Account<'info, StreamAccount>,

    /// CHECK: Vault PDA holds SOL escrow; validated by seeds
    #[account(
        mut,
        seeds = [b"vault", stream_account.key().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,

    #[account(mut)]
    pub funder: Signer<'info>,

    /// CHECK: Verified as valid Solana address for milestone receiver
    pub beneficiary: AccountInfo<'info>,

    /// CHECK: Verified as authorized evaluation signer
    pub evaluator: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitMilestone<'info> {
    #[account(
        mut,
        has_one = beneficiary @ VibeStreamError::UnauthorizedSigner,
    )]
    pub stream_account: Account<'info, StreamAccount>,

    pub beneficiary: Signer<'info>,
}

#[derive(Accounts)]
pub struct ApproveMilestone<'info> {
    #[account(
        mut,
        has_one = evaluator @ VibeStreamError::UnauthorizedSigner,
        has_one = beneficiary @ VibeStreamError::InvalidBeneficiary,
    )]
    pub stream_account: Account<'info, StreamAccount>,

    /// CHECK: Checked by seeds against stream_account
    #[account(
        mut,
        seeds = [b"vault", stream_account.key().as_ref()],
        bump = stream_account.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    pub evaluator: Signer<'info>,

    /// CHECK: Receives released lamports; matches stream_account.beneficiary
    #[account(mut)]
    pub beneficiary: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EmergencyClawback<'info> {
    #[account(
        mut,
        has_one = funder @ VibeStreamError::UnauthorizedSigner,
    )]
    pub stream_account: Account<'info, StreamAccount>,

    /// CHECK: Checked by seeds against stream_account
    #[account(
        mut,
        seeds = [b"vault", stream_account.key().as_ref()],
        bump = stream_account.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    #[account(mut)]
    pub funder: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// STATE STORAGE
// -----------------------------------------------------------------------------

#[account]
pub struct StreamAccount {
    pub funder: Pubkey,                     // 32
    pub beneficiary: Pubkey,                // 32
    pub evaluator: Pubkey,                  // 32
    pub stream_id: u64,                     // 8
    pub total_amount: u64,                  // 8
    pub remaining_balance: u64,             // 8
    pub timelock_deadline: i64,             // 8
    pub total_milestones: u8,               // 1
    pub approved_milestones: u8,            // 1
    pub current_milestone_submitted: bool,  // 1
    pub is_active: bool,                    // 1
    pub is_cancelled: bool,                 // 1
    pub bump: u8,                           // 1
    pub vault_bump: u8,                     // 1
    pub metadata_len: u8,                   // 1
    pub metadata_uri: [u8; MAX_METADATA_LEN], // 96
}

impl StreamAccount {
    // 8 (discriminator) + 32*3 + 8*3 + 1*8 + 96 = 224
    pub const LEN: usize = 8 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + MAX_METADATA_LEN;
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct StreamCreatedEvent {
    pub stream_pda: Pubkey,
    pub stream_id: u64,
    pub funder: Pubkey,
    pub beneficiary: Pubkey,
    pub evaluator: Pubkey,
    pub total_amount: u64,
    pub total_milestones: u8,
    pub timelock_deadline: i64,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub stream_pda: Pubkey,
    pub stream_id: u64,
    pub milestone_index: u8,
    pub proof_uri: String,
    pub beneficiary: Pubkey,
}

#[event]
pub struct MilestoneApprovedEvent {
    pub stream_pda: Pubkey,
    pub stream_id: u64,
    pub milestone_index: u8,
    pub amount_unlocked: u64,
    pub remaining_balance: u64,
    pub beneficiary: Pubkey,
    pub is_completed: bool,
}

#[event]
pub struct StreamClawedBackEvent {
    pub stream_pda: Pubkey,
    pub stream_id: u64,
    pub funder: Pubkey,
    pub amount_reclaimed: u64,
}

// -----------------------------------------------------------------------------
// CUSTOM ERRORS
// -----------------------------------------------------------------------------

#[error_code]
pub enum VibeStreamError {
    #[msg("Initial funding amount must be strictly greater than zero.")]
    ZeroFundingAmount,
    #[msg("Total milestones must be between 1 and 10.")]
    InvalidMilestoneCount,
    #[msg("Provided metadata URI exceeds maximum capacity (96 bytes).")]
    MetadataUriTooLong,
    #[msg("Timelock expiration deadline must be set in the future.")]
    InvalidTimelockDeadline,
    #[msg("Signer is not authorized to execute this state transition.")]
    UnauthorizedSigner,
    #[msg("Beneficiary account mismatch.")]
    InvalidBeneficiary,
    #[msg("Target stream is inactive.")]
    StreamInactive,
    #[msg("Target stream has been cancelled or clawed back.")]
    StreamCancelled,
    #[msg("Milestones must be submitted and approved strictly in sequential order.")]
    SequentialMilestoneViolation,
    #[msg("Current milestone deliverable has already been submitted.")]
    MilestoneAlreadySubmitted,
    #[msg("Milestone deliverable proof has not been submitted yet.")]
    MilestoneNotSubmitted,
    #[msg("Clawback cannot be invoked before the emergency timelock deadline has expired.")]
    TimelockNotExpired,
    #[msg("Calculated release amount violates vault balance invariant.")]
    ArithmeticOverflow,
}