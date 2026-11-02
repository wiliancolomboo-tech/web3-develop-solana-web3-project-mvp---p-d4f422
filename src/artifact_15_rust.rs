use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("UniStrm1111111111111111111111111111111111111");

#[program]
pub mod unistream {
    use super::*;

    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        grant_id: u64,
        title: String,
        milestone_amounts: Vec<u64>,
    ) -> Result<()> {
        instructions::initialize_grant::handler(ctx, grant_id, title, milestone_amounts)
    }

    pub fn deposit_funds(ctx: Context<DepositFunds>, amount: u64) -> Result<()> {
        instructions::deposit_funds::handler(ctx, amount)
    }

    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        proof_hash: [u8; 32],
        proof_uri: String,
    ) -> Result<()> {
        instructions::submit_milestone::handler(ctx, milestone_index, proof_hash, proof_uri)
    }

    pub fn approve_milestone(ctx: Context<ApproveMilestone>, milestone_index: u8) -> Result<()> {
        instructions::approve_milestone::handler(ctx, milestone_index)
    }

    pub fn claim_funds(ctx: Context<ClaimFunds>, milestone_index: u8) -> Result<()> {
        instructions::claim_funds::handler(ctx, milestone_index)
    }

    pub fn cancel_grant(ctx: Context<CancelGrant>) -> Result<()> {
        instructions::cancel_grant::handler(ctx)
    }
}