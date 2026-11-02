use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::UniVibeError;
use crate::events::MilestoneApprovedEvent;

#[derive(Accounts)]
pub struct ApproveMilestone<'info> {
    #[account(
        mut,
        has_one = reviewer @ UniVibeError::Unauthorized,
        constraint = grant.is_active @ UniVibeError::GrantClosed
    )]
    pub grant: Account<'info, Grant>,

    pub reviewer: Signer<'info>,
}

pub fn handler(ctx: Context<ApproveMilestone>, milestone_index: u8) -> Result<()> {
    let grant = &mut ctx.accounts.grant;
    let idx = milestone_index as usize;

    require!(idx < grant.milestone_count as usize, UniVibeError::InvalidMilestoneIndex);
    require!(
        grant.milestones[idx].status == MilestoneStatus::UnderReview,
        UniVibeError::MilestoneNotPending
    );

    let clock = Clock::get()?;
    grant.milestones[idx].status = MilestoneStatus::Approved;
    grant.milestones[idx].approved_at = clock.unix_timestamp;

    emit!(MilestoneApprovedEvent {
        grant_pubkey: grant.key(),
        milestone_index,
        reviewer: ctx.accounts.reviewer.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}