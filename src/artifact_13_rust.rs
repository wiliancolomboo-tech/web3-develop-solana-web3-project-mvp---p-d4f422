use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("UniVibe111111111111111111111111111111111111");

#[program]
pub mod univibe {
    use super::*;

    pub fn initialize_round(
        ctx: Context<InitializeRound>,
        round_index: u64,
        matching_pool_lamports: u64,
        start_time: i64,
        end_time: i64,
    ) -> Result<()> {
        instructions::initialize_round::handler(ctx, round_index, matching_pool_lamports, start_time, end_time)
    }

    pub fn submit_proposal(
        ctx: Context<SubmitProposal>,
        title: String,
        requested_amount_lamports: u64,
        milestone_shares: Vec<u8>,
    ) -> Result<()> {
        instructions::submit_proposal::handler(ctx, title, requested_amount_lamports, milestone_shares)
    }

    pub fn cast_vote(
        ctx: Context<CastVote>,
        vote_credits_lamports: u64,
    ) -> Result<()> {
        instructions::cast_vote::handler(ctx, vote_credits_lamports)
    }

    pub fn approve_milestone(
        ctx: Context<ApproveMilestone>,
        milestone_index: u8,
    ) -> Result<()> {
        instructions::approve_milestone::handler(ctx, milestone_index)
    }

    pub fn claim_milestone_funds(
        ctx: Context<ClaimMilestoneFunds>,
        milestone_index: u8,
    ) -> Result<()> {
        instructions::claim_funds::handler(ctx, milestone_index)
    }
}