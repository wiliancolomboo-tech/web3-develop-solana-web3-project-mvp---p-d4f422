use anchor_lang::prelude::*;
use anchor_lang::system_program;

declare_id!("V1bE9f9Z4uNqWk6Zk3P9V8Q7X2Y1Z0aBcDeFgHiJkLm");

pub const MAX_MILESTONES: usize = 5;
pub const MAX_URI_LENGTH: usize = 128;
pub const MAX_TITLE_LENGTH: usize = 64;

#[program]
pub mod univibe_core {
    use super::*;

    /// Initializes a new Milestone Grant Vault with escrowed native lamports
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        grant_id: u64,
        title: String,
        milestone_amounts: Vec<u64>,
        deadline_timestamps: Vec<i64>,
    ) -> Result<()> {
        require!(
            !title.is_empty() && title.len() <= MAX_TITLE_LENGTH,
            GrantError::InvalidTitleLength
        );
        require!(
            !milestone_amounts.is_empty() && milestone_amounts.len() <= MAX_MILESTONES,
            GrantError::InvalidMilestoneCount
        );
        require!(
            milestone_amounts.len() == deadline_timestamps.len(),
            GrantError::MilestoneParameterMismatch
        );

        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;

        let mut total_escrow: u64 = 0;
        let mut milestones: Vec<Milestone> = Vec::new();

        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts[i];
            let deadline = deadline_timestamps[i];

            require!(amount > 0, GrantError::ZeroAmountDisallowed);
            require!(
                deadline > current_time,
                GrantError::DeadlineMustBeInFuture
            );

            if i > 0 {
                require!(
                    deadline >= deadline_timestamps[i - 1],
                    GrantError::DeadlinesMustBeSequential
                );
            }

            total_escrow = total_escrow
                .checked_add(amount)
                .ok_or(GrantError::ArithmeticOverflow)?;

            milestones.push(Milestone {
                amount,
                deadline,
                status: MilestoneStatus::Initialized,
                deliverable_uri: [0u8; MAX_URI_LENGTH],
                uri_len: 0,
                approved_at: 0,
            });
        }

        // Deposit initial funding from sponsor into vault PDA
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.sponsor.to_account_info(),
                to: ctx.accounts.vault_pda.to_account_info(),
            },
        );
        system_program::transfer(cpi_context, total_escrow)?;

        let grant = &mut ctx.accounts.grant_account;
        grant.grant_id = grant_id;
        grant.sponsor = ctx.accounts.sponsor.key();
        grant.builder = ctx.accounts.builder.key();
        grant.evaluator = ctx.accounts.evaluator.key();
        grant.vault_bump = ctx.bumps.vault_pda;
        grant.state_bump = ctx.bumps.grant_account;
        grant.total_allocated = total_escrow;
        grant.total_released = 0;
        grant.milestone_count = milestones.len() as u8;
        grant.is_canceled = false;
        grant.created_at = current_time;

        for (idx, m) in milestones.into_iter().enumerate() {
            grant.milestones[idx] = m;
        }

        emit!(GrantInitializedEvent {
            grant_id,
            sponsor: grant.sponsor,
            builder: grant.builder,
            evaluator: grant.evaluator,
            total_allocated: total_escrow,
            created_at: current_time,
        });

        Ok(())
    }

    /// Builder submits verification deliverable proof (IPFS/Arweave Hash)
    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        deliverable_uri: String,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        require!(!grant.is_canceled, GrantError::GrantIsCanceled);
        require!(
            (milestone_index as usize) < (grant.milestone_count as usize),
            GrantError::InvalidMilestoneIndex
        );

        let milestone = &mut grant.milestones[milestone_index as usize];
        require!(
            milestone.status == MilestoneStatus::Initialized,
            GrantError::InvalidMilestoneStateTransition
        );

        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp <= milestone.deadline,
            GrantError::MilestoneExpired
        );

        let uri_bytes = deliverable_uri.as_bytes();
        require!(
            !uri_bytes.is_empty() && uri_bytes.len() <= MAX_URI_LENGTH,
            GrantError::InvalidDeliverableLength
        );

        milestone.uri_len = uri_bytes.len() as u8;
        milestone.deliverable_uri[..uri_bytes.len()].copy_from_slice(uri_bytes);
        milestone.status = MilestoneStatus::Submitted;

        emit!(MilestoneSubmittedEvent {
            grant_id: grant.grant_id,
            milestone_index,
            builder: grant.builder,
            deliverable_uri,
            submitted_at: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Appointed Evaluator (Hackathon Judge / DAO Oracle) reviews and approves milestone
    pub fn approve_milestone(
        ctx: Context<ApproveMilestone>,
        milestone_index: u8,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        require!(!grant.is_canceled, GrantError::GrantIsCanceled);
        require!(
            (milestone_index as usize) < (grant.milestone_count as usize),
            GrantError::InvalidMilestoneIndex
        );

        let milestone = &mut grant.milestones[milestone_index as usize];
        require!(
            milestone.status == MilestoneStatus::Submitted,
            GrantError::MilestoneNotSubmitted
        );

        let clock = Clock::get()?;
        milestone.status = MilestoneStatus::Approved;
        milestone.approved_at = clock.unix_timestamp;

        emit!(MilestoneApprovedEvent {
            grant_id: grant.grant_id,
            milestone_index,
            evaluator: ctx.accounts.evaluator.key(),
            amount: milestone.amount,
            approved_at: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Autonomous payout settlement: transfers funds from Vault PDA to Builder
    pub fn claim_payout(
        ctx: Context<ClaimPayout>,
        milestone_index: u8,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        require!(!grant.is_canceled, GrantError::GrantIsCanceled);
        require!(
            (milestone_index as usize) < (grant.milestone_count as usize),
            GrantError::InvalidMilestoneIndex
        );

        let milestone = &mut grant.milestones[milestone_index as usize];
        require!(
            milestone.status == MilestoneStatus::Approved,
            GrantError::MilestoneNotApproved
        );

        let payout_amount = milestone.amount;
        require!(
            payout_amount <= ctx.accounts.vault_pda.to_account_info().lamports(),
            GrantError::InsufficientVaultBalance
        );

        milestone.status = MilestoneStatus::Claimed;
        grant.total_released = grant
            .total_released
            .checked_add(payout_amount)
            .ok_or(GrantError::ArithmeticOverflow)?;

        // Execute CPI from PDA with Seeds
        let grant_id_bytes = grant.grant_id.to_le_bytes();
        let seeds: &[&[&[u8]]] = &[&[
            b"univibe_vault",
            grant_id_bytes.as_ref(),
            &[grant.vault_bump],
        ]];

        **ctx.accounts.vault_pda.to_account_info().try_borrow_mut_lamports()? = ctx
            .accounts
            .vault_pda
            .to_account_info()
            .lamports()
            .checked_sub(payout_amount)
            .ok_or(GrantError::ArithmeticOverflow)?;

        **ctx.accounts.builder.to_account_info().try_borrow_mut_lamports()? = ctx
            .accounts
            .builder
            .to_account_info()
            .lamports()
            .checked_add(payout_amount)
            .ok_or(GrantError::ArithmeticOverflow)?;

        emit!(PayoutClaimedEvent {
            grant_id: grant.grant_id,
            milestone_index,
            builder: ctx.accounts.builder.key(),
            amount: payout_amount,
            claimed_at: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Sponsor emergency recovery: claw back unclaimed or expired milestone allocations
    pub fn emergency_clawback(ctx: Context<EmergencyClawback>) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;

        let mut reclaimable_amount: u64 = 0;
        for i in 0..(grant.milestone_count as usize) {
            let milestone = &mut grant.milestones[i];
            if milestone.status == MilestoneStatus::Initialized
                || milestone.status == MilestoneStatus::Submitted
            {
                if current_time > milestone.deadline {
                    milestone.status = MilestoneStatus::Expired;
                    reclaimable_amount = reclaimable_amount
                        .checked_add(milestone.amount)
                        .ok_or(GrantError::ArithmeticOverflow)?;
                }
            }
        }

        require!(reclaimable_amount > 0, GrantError::NoFundsEligibleForClawback);

        let vault_balance = ctx.accounts.vault_pda.to_account_info().lamports();
        let final_refund = std::cmp::min(reclaimable_amount, vault_balance);

        **ctx.accounts.vault_pda.to_account_info().try_borrow_mut_lamports()? = vault_balance
            .checked_sub(final_refund)
            .ok_or(GrantError::ArithmeticOverflow)?;

        **ctx.accounts.sponsor.to_account_info().try_borrow_mut_lamports()? = ctx
            .accounts
            .sponsor
            .to_account_info()
            .lamports()
            .checked_add(final_refund)
            .ok_or(GrantError::ArithmeticOverflow)?;

        emit!(ClawbackExecutedEvent {
            grant_id: grant.grant_id,
            sponsor: ctx.accounts.sponsor.key(),
            refunded_amount: final_refund,
            executed_at: current_time,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct InitializeGrant<'info> {
    #[account(mut)]
    pub sponsor: Signer<'info>,

    /// CHECK: Target recipient student builder public key
    pub builder: AccountInfo<'info>,

    /// CHECK: Appointed evaluator/hackathon judge authority
    pub evaluator: AccountInfo<'info>,

    #[account(
        init,
        payer = sponsor,
        space = 8 + GrantAccount::INIT_SPACE,
        seeds = [b"univibe_state", grant_id.to_le_bytes().as_ref()],
        bump
    )]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [b"univibe_vault", grant_id.to_le_bytes().as_ref()],
        bump
    )]
    pub vault_pda: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitMilestone<'info> {
    #[account(
        mut,
        has_one = builder @ GrantError::UnauthorizedBuilder,
        seeds = [b"univibe_state", grant_account.grant_id.to_le_bytes().as_ref()],
        bump = grant_account.state_bump
    )]
    pub grant_account: Account<'info, GrantAccount>,

    pub builder: Signer<'info>,
}

#[derive(Accounts)]
pub struct ApproveMilestone<'info> {
    #[account(
        mut,
        has_one = evaluator @ GrantError::UnauthorizedEvaluator,
        seeds = [b"univibe_state", grant_account.grant_id.to_le_bytes().as_ref()],
        bump = grant_account.state_bump
    )]
    pub grant_account: Account<'info, GrantAccount>,

    pub evaluator: Signer<'info>,
}

#[derive(Accounts)]
pub struct ClaimPayout<'info> {
    #[account(
        mut,
        has_one = builder @ GrantError::UnauthorizedBuilder,
        seeds = [b"univibe_state", grant_account.grant_id.to_le_bytes().as_ref()],
        bump = grant_account.state_bump
    )]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [b"univibe_vault", grant_account.grant_id.to_le_bytes().as_ref()],
        bump = grant_account.vault_bump
    )]
    pub vault_pda: SystemAccount<'info>,

    #[account(mut)]
    pub builder: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EmergencyClawback<'info> {
    #[account(
        mut,
        has_one = sponsor @ GrantError::UnauthorizedSponsor,
        seeds = [b"univibe_state", grant_account.grant_id.to_le_bytes().as_ref()],
        bump = grant_account.state_bump
    )]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [b"univibe_vault", grant_account.grant_id.to_le_bytes().as_ref()],
        bump = grant_account.vault_bump
    )]
    pub vault_pda: SystemAccount<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// STATE DEFINITIONS
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct GrantAccount {
    pub grant_id: u64,
    pub sponsor: Pubkey,
    pub builder: Pubkey,
    pub evaluator: Pubkey,
    pub vault_bump: u8,
    pub state_bump: u8,
    pub total_allocated: u64,
    pub total_released: u64,
    pub milestone_count: u8,
    pub is_canceled: bool,
    pub created_at: i64,
    pub milestones: [Milestone; MAX_MILESTONES],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Default)]
pub struct Milestone {
    pub amount: u64,
    pub deadline: i64,
    pub status: MilestoneStatus,
    pub deliverable_uri: [u8; MAX_URI_LENGTH],
    pub uri_len: u8,
    pub approved_at: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Default)]
pub enum MilestoneStatus {
    #[default]
    Initialized,
    Submitted,
    Approved,
    Claimed,
    Expired,
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantInitializedEvent {
    pub grant_id: u64,
    pub sponsor: Pubkey,
    pub builder: Pubkey,
    pub evaluator: Pubkey,
    pub total_allocated: u64,
    pub created_at: i64,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub grant_id: u64,
    pub milestone_index: u8,
    pub builder: Pubkey,
    pub deliverable_uri: String,
    pub submitted_at: i64,
}

#[event]
pub struct MilestoneApprovedEvent {
    pub grant_id: u64,
    pub milestone_index: u8,
    pub evaluator: Pubkey,
    pub amount: u64,
    pub approved_at: i64,
}

#[event]
pub struct PayoutClaimedEvent {
    pub grant_id: u64,
    pub milestone_index: u8,
    pub builder: Pubkey,
    pub amount: u64,
    pub claimed_at: i64,
}

#[event]
pub struct ClawbackExecutedEvent {
    pub grant_id: u64,
    pub sponsor: Pubkey,
    pub refunded_amount: u64,
    pub executed_at: i64,
}

// -----------------------------------------------------------------------------
// ERROR CODES
// -----------------------------------------------------------------------------

#[error_code]
pub enum GrantError {
    #[msg("Grant title is empty or exceeds the maximum permitted length.")]
    InvalidTitleLength,
    #[msg("Invalid milestone count; must be between 1 and 5.")]
    InvalidMilestoneCount,
    #[msg("Mismatched counts between milestone amounts and submission deadlines.")]
    MilestoneParameterMismatch,
    #[msg("Milestone funding allocation must be greater than zero.")]
    ZeroAmountDisallowed,
    #[msg("Milestone deadline must be set in the future.")]
    DeadlineMustBeInFuture,
    #[msg("Sequential deadline rule violated; deadlines must increase monotonically.")]
    DeadlinesMustBeSequential,
    #[msg("Math calculation resulted in an arithmetic overflow.")]
    ArithmeticOverflow,
    #[msg("Specified milestone index is outside allowable bounds.")]
    InvalidMilestoneIndex,
    #[msg("Signer is not authorized as the grant builder.")]
    UnauthorizedBuilder,
    #[msg("Signer is not authorized as the grant evaluator.")]
    UnauthorizedEvaluator,
    #[msg("Signer is not authorized as the grant sponsor.")]
    UnauthorizedSponsor,
    #[msg("Milestone has already progressed past initialization.")]
    InvalidMilestoneStateTransition,
    #[msg("Submission deadline for this milestone has elapsed.")]
    MilestoneExpired,
    #[msg("Deliverable URI must not be empty and cannot exceed maximum length.")]
    InvalidDeliverableLength,
    #[msg("Milestone deliverable has not been submitted by builder.")]
    MilestoneNotSubmitted,
    #[msg("Milestone has not been approved by the designated evaluator.")]
    MilestoneNotApproved,
    #[msg("Grant vault holds insufficient balance for requested distribution.")]
    InsufficientVaultBalance,
    #[msg("No milestones meet the condition for timeout clawback.")]
    NoFundsEligibleForClawback,
    #[msg("The grant escrow has been canceled.")]
    GrantIsCanceled,
}