// programs/univibe_protocol/src/lib.rs
use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("VibeGRNT11111111111111111111111111111111111");

#[program]
pub mod univibe_protocol {
    use super::*;

    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        title: String,
        milestone_amounts: Vec<u64>,
    ) -> Result<()> {
        instructions::initialize_grant::handler(ctx, title, milestone_amounts)
    }

    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        deliverable_uri: String,
    ) -> Result<()> {
        instructions::submit_milestone::handler(ctx, milestone_index, deliverable_uri)
    }

    pub fn approve_milestone(
        ctx: Context<ApproveMilestone>,
        milestone_index: u8,
    ) -> Result<()> {
        instructions::approve_milestone::handler(ctx, milestone_index)
    }

    pub fn release_funds(
        ctx: Context<ReleaseFunds>,
        milestone_index: u8,
    ) -> Result<()> {
        instructions::release_funds::handler(ctx, milestone_index)
    }
}