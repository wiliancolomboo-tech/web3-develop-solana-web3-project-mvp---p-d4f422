use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use crate::state::*;
use crate::errors::UniVibeError;
use crate::events::GrantCreatedEvent;

#[derive(Accounts)]
#[instruction(title: String, milestone_amounts: Vec<u64>)]
pub struct InitializeGrant<'info> {
    #[account(
        init,
        payer = sponsor,
        space = Grant::LEN,
        seeds = [b"grant", sponsor.key().as_ref(), title.as_bytes()],
        bump
    )]
    pub grant: Account<'info, Grant>,

    /// CHECK: PDA escrow vault that holds Native SOL deposits
    #[account(
        mut,
        seeds = [b"escrow", grant.key().as_ref()],
        bump
    )]
    pub escrow_vault: SystemAccount<'info>,

    /// Beneficiary student receiving milestones
    /// CHECK: Validated by address specification
    pub beneficiary: AccountInfo<'info>,

    /// University club lead / reviewer authorized to audit deliverables
    /// CHECK: Validated by address specification
    pub reviewer: AccountInfo<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializeGrant>,
    title: String,
    milestone_amounts: Vec<u64>,
) -> Result<()> {
    require!(title.len() <= MAX_TITLE_LEN, UniVibeError::EmptyDeliverableUri);
    let count = milestone_amounts.len();
    require!(count > 0 && count <= MAX_MILESTONES, UniVibeError::ExceededMaxMilestones);

    let total_amount: u64 = milestone_amounts.iter().sum();
    require!(total_amount > 0, UniVibeError::MilestoneAllocationMismatch);

    let grant = &mut ctx.accounts.grant;
    grant.sponsor = ctx.accounts.sponsor.key();
    grant.beneficiary = ctx.accounts.beneficiary.key();
    grant.reviewer = ctx.accounts.reviewer.key();
    grant.total_amount = total_amount;
    grant.claimed_amount = 0;
    grant.milestone_count = count as u8;
    grant.is_active = true;
    grant.bump = ctx.bumps.grant;
    grant.escrow_bump = ctx.bumps.escrow_vault;
    grant.title = title.clone();

    for i in 0..MAX_MILESTONES {
        if i < count {
            grant.milestones[i] = Milestone {
                amount: milestone_amounts[i],
                status: MilestoneStatus::PendingSubmission,
                submitted_at: 0,
                approved_at: 0,
            };
        } else {
            grant.milestones[i] = Milestone::default();
        }
    }

    // Transfer total fund into PDA escrow vault
    let transfer_ctx = CpiContext::new(
        ctx.accounts.system_program.to_account_info(),
        Transfer {
            from: ctx.accounts.sponsor.to_account_info(),
            to: ctx.accounts.escrow_vault.to_account_info(),
        },
    );
    transfer(transfer_ctx, total_amount)?;

    emit!(GrantCreatedEvent {
        grant_pubkey: grant.key(),
        sponsor: grant.sponsor,
        beneficiary: grant.beneficiary,
        reviewer: grant.reviewer,
        total_amount,
        milestone_count: count as u8,
        title,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}