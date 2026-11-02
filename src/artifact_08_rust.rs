use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::UniVibeError;
use crate::events::MilestoneSubmittedEvent;

#[derive(Accounts)]
pub struct SubmitMilestone<'info> {
    #[account(
        mut,
        has_one = beneficiary @ UniVibeError::Unauthorized,
        constraint = grant.is_active @ UniVibeError::GrantClosed
    )]
    pub grant: Account<'info, Grant>,

    pub beneficiary: Signer<'info>,
}

pub fn handler(
    ctx: Context<SubmitMilestone>,
    milestone_index: u8,
    deliverable_uri: String,
) -> Result<()> {
    require!(!deliverable_uri.trim().is_empty(), UniVibeError::EmptyDeliverableUri);
    let grant = &mut ctx.accounts.grant;
    let idx = milestone_index as usize;

    require!(idx < grant.milestone_count as usize, UniVibeError::InvalidMilestoneIndex);
    require!(
        grant.milestones[idx].status == MilestoneStatus::PendingSubmission,
        UniVibeError::InvalidMilestoneStateForSubmission
    );

    let clock = Clock::get()?;
    grant.milestones[idx].status = MilestoneStatus::UnderReview;
    grant.milestones[idx].submitted_at = clock.unix_timestamp;

    emit!(MilestoneSubmittedEvent {
        grant_pubkey: grant.key(),
        milestone_index,
        deliverable_uri,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}