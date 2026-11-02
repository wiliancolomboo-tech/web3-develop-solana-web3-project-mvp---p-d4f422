use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::UniVibeError;
use crate::events::MilestoneFundsReleasedEvent;

#[derive(Accounts)]
pub struct ReleaseFunds<'info> {
    #[account(
        mut,
        has_one = beneficiary @ UniVibeError::Unauthorized,
        constraint = grant.is_active @ UniVibeError::GrantClosed
    )]
    pub grant: Account<'info, Grant>,

    /// CHECK: PDA escrow account that stores SOL
    #[account(
        mut,
        seeds = [b"escrow", grant.key().as_ref()],
        bump = grant.escrow_bump
    )]
    pub escrow_vault: SystemAccount<'info>,

    /// Beneficiary student wallet
    #[account(mut)]
    /// CHECK: Validated against grant.beneficiary
    pub beneficiary: AccountInfo<'info>,

    /// Caller can be student, sponsor, or reviewer
    pub caller: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<ReleaseFunds>, milestone_index: u8) -> Result<()> {
    let grant = &mut ctx.accounts.grant;
    let idx = milestone_index as usize;

    require!(idx < grant.milestone_count as usize, UniVibeError::InvalidMilestoneIndex);
    require!(
        grant.milestones[idx].status == MilestoneStatus::Approved,
        UniVibeError::MilestoneNotApproved
    );

    let payout_amount = grant.milestones[idx].amount;
    grant.milestones[idx].status = MilestoneStatus::Claimed;
    grant.claimed_amount = grant.claimed_amount.checked_add(payout_amount).unwrap();

    if grant.claimed_amount >= grant.total_amount {
        grant.is_active = false;
    }

    // Direct PDA transfer via Lamport balance adjustment
    let escrow_info = ctx.accounts.escrow_vault.to_account_info();
    let beneficiary_info = ctx.accounts.beneficiary.to_account_info();

    **escrow_info.try_borrow_mut_lamports()? = escrow_info
        .lamports()
        .checked_sub(payout_amount)
        .ok_or(ProgramError::InsufficientFunds)?;

    **beneficiary_info.try_borrow_mut_lamports()? = beneficiary_info
        .lamports()
        .checked_add(payout_amount)
        .ok_or(ProgramError::InvalidAccountData)?;

    emit!(MilestoneFundsReleasedEvent {
        grant_pubkey: grant.key(),
        milestone_index,
        beneficiary: grant.beneficiary,
        amount: payout_amount,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}