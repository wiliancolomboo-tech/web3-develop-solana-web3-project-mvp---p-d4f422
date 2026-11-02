use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("UniVibe1111111111111111111111111111111111111");

#[program]
pub mod univibe_protocol {
    use super::*;

    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        grant_id: u64,
        payout_schedule: Vec<u64>,
        reviewers: Vec<Pubkey>,
        quorum_threshold: u8,
        expiration_timelock: i64,
    ) -> Result<()> {
        instructions::initialize_grant::handler(
            ctx,
            grant_id,
            payout_schedule,
            reviewers,
            quorum_threshold,
            expiration_timelock,
        )
    }

    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        proof_uri: String,
    ) -> Result<()> {
        instructions::submit_milestone::handler(ctx, milestone_index, proof_uri)
    }

    pub fn vote_milestone(
        ctx: Context<VoteMilestone>,
        milestone_index: u8,
    ) -> Result<()> {
        instructions::vote_milestone::handler(ctx, milestone_index)
    }

    pub fn release_milestone(
        ctx: Context<ReleaseMilestone>,
        milestone_index: u8,
    ) -> Result<()> {
        instructions::release_milestone::handler(ctx, milestone_index)
    }

    pub fn refund_stale_grant(ctx: Context<RefundStaleGrant>) -> Result<()> {
        instructions::refund_stale_grant::handler(ctx)
    }
}