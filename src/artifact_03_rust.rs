use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("CampFlow11111111111111111111111111111111111");

pub const DISCRIMINATOR_LENGTH: usize = 8;
pub const MAX_TITLE_LENGTH: usize = 64;
pub const MAX_URI_LENGTH: usize = 128;
pub const MIN_VOUCHES_REQUIRED: u32 = 3;

#[program]
pub mod campus_flow {
    use super::*;

    /// Initializes a new Hackathon Grant escrow pool with SPL token collateral
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        grant_id: u64,
        title: String,
        total_budget: u64,
    ) -> Result<()> {
        require!(title.len() <= MAX_TITLE_LENGTH, CampusFlowError::TitleTooLong);
        require!(total_budget > 0, CampusFlowError::InvalidBudget);

        let grant = &mut ctx.accounts.grant;
        grant.authority = ctx.accounts.sponsor.key();
        grant.mint = ctx.accounts.mint.key();
        grant.vault = ctx.accounts.vault.key();
        grant.grant_id = grant_id;
        grant.total_budget = total_budget;
        grant.disbursed_amount = 0;
        grant.title = title;
        grant.bump = ctx.bumps.grant;
        grant.vault_bump = ctx.bumps.vault;
        grant.created_at = Clock::get()?.unix_timestamp;

        // Transfer funds from sponsor into Escrow PDA Vault
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sponsor_token_account.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.sponsor.to_account_info(),
            },
        );
        token::transfer(transfer_ctx, total_budget)?;

        emit!(GrantInitializedEvent {
            grant_id,
            sponsor: ctx.accounts.sponsor.key(),
            total_budget,
            timestamp: grant.created_at,
        });

        Ok(())
    }

    /// Registers a student profile using an authorized authority attestation
    pub fn register_student(
        ctx: Context<RegisterStudent>,
        institution_hash: [u8; 32],
    ) -> Result<()> {
        let student = &mut ctx.accounts.student_profile;
        student.authority = ctx.accounts.student.key();
        student.institution_hash = institution_hash;
        student.vouch_count = 0;
        student.reputation_score = 100; // Base score
        student.bump = ctx.bumps.student_profile;
        student.registered_at = Clock::get()?.unix_timestamp;

        emit!(StudentRegisteredEvent {
            student: ctx.accounts.student.key(),
            institution_hash,
            timestamp: student.registered_at,
        });

        Ok(())
    }

    /// Builder submits a project milestone for delivery verification
    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        deliverable_uri: String,
        payout_amount: u64,
    ) -> Result<()> {
        require!(deliverable_uri.len() <= MAX_URI_LENGTH, CampusFlowError::UriTooLong);
        require!(payout_amount > 0, CampusFlowError::InvalidAmount);

        let grant = &ctx.accounts.grant;
        let remaining_budget = grant.total_budget
            .checked_sub(grant.disbursed_amount)
            .ok_or(CampusFlowError::MathOverflow)?;

        require!(payout_amount <= remaining_budget, CampusFlowError::InsufficientGrantFunds);

        let milestone = &mut ctx.accounts.milestone;
        milestone.grant = grant.key();
        milestone.builder = ctx.accounts.student_profile.key();
        milestone.milestone_index = milestone_index;
        milestone.payout_amount = payout_amount;
        milestone.deliverable_uri = deliverable_uri;
        milestone.vouch_count = 0;
        milestone.is_completed = false;
        milestone.is_disbursed = false;
        milestone.bump = ctx.bumps.milestone;
        milestone.submitted_at = Clock::get()?.unix_timestamp;

        emit!(MilestoneSubmittedEvent {
            grant: grant.key(),
            builder: ctx.accounts.student_profile.key(),
            milestone_index,
            payout_amount,
        });

        Ok(())
    }

    /// Peer vouches for a milestone to prove genuine collegiate building (Proof-of-Vibe)
    pub fn cast_vouch(ctx: Context<CastVouch>) -> Result<()> {
        require!(
            ctx.accounts.voter_profile.authority != ctx.accounts.builder_profile.authority,
            CampusFlowError::SelfVouchingNotAllowed
        );

        let milestone = &mut ctx.accounts.milestone;
        require!(!milestone.is_completed, CampusFlowError::MilestoneAlreadyCompleted);

        milestone.vouch_count = milestone.vouch_count.checked_add(1).ok_or(CampusFlowError::MathOverflow)?;
        
        let voter = &mut ctx.accounts.voter_profile;
        voter.vouch_count = voter.vouch_count.checked_add(1).ok_or(CampusFlowError::MathOverflow)?;

        emit!(VouchCastEvent {
            milestone: milestone.key(),
            voter: voter.key(),
            total_vouches: milestone.vouch_count,
        });

        Ok(())
    }

    /// Sponsor approves milestone and triggers immediate SPL token release from PDA vault
    pub fn release_milestone(
        ctx: Context<ReleaseMilestone>,
        grant_id: u64,
    ) -> Result<()> {
        let milestone = &mut ctx.accounts.milestone;
        let grant = &mut ctx.accounts.grant;

        require!(!milestone.is_disbursed, CampusFlowError::AlreadyDisbursed);
        require!(milestone.vouch_count >= MIN_VOUCHES_REQUIRED, CampusFlowError::NotEnoughVouches);

        // State mutations before CPI
        milestone.is_completed = true;
        milestone.is_disbursed = true;

        grant.disbursed_amount = grant.disbursed_amount
            .checked_add(milestone.payout_amount)
            .ok_or(CampusFlowError::MathOverflow)?;

        // PDA Signer seeds for Vault authorization
        let grant_id_bytes = grant_id.to_le_bytes();
        let seeds = &[
            b"vault".as_ref(),
            grant.to_account_info().key.as_ref(),
            &[grant.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        // Execute CPI token transfer from PDA to Student ATA
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.builder_token_account.to_account_info(),
                authority: ctx.accounts.vault.to_account_info(),
            },
            signer_seeds,
        );
        token::transfer(transfer_ctx, milestone.payout_amount)?;

        emit!(MilestoneReleasedEvent {
            grant: grant.key(),
            milestone: milestone.key(),
            builder: ctx.accounts.builder.key(),
            amount: milestone.payout_amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT INSTRUCTION STRUCTURES
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct InitializeGrant<'info> {
    #[account(mut)]
    pub sponsor: Signer<'info>,

    #[account(
        init,
        payer = sponsor,
        space = DISCRIMINATOR_LENGTH + Grant::INIT_SPACE,
        seeds = [b"grant", sponsor.key().as_ref(), &grant_id.to_le_bytes()],
        bump
    )]
    pub grant: Account<'info, Grant>,

    pub mint: Account<'info, Mint>,

    #[account(
        init,
        payer = sponsor,
        seeds = [b"vault", grant.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = vault
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = sponsor_token_account.mint == mint.key(),
        constraint = sponsor_token_account.owner == sponsor.key()
    )]
    pub sponsor_token_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct RegisterStudent<'info> {
    #[account(mut)]
    pub student: Signer<'info>,

    #[account(
        init,
        payer = student,
        space = DISCRIMINATOR_LENGTH + StudentProfile::INIT_SPACE,
        seeds = [b"student", student.key().as_ref()],
        bump
    )]
    pub student_profile: Account<'info, StudentProfile>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_index: u8)]
pub struct SubmitMilestone<'info> {
    #[account(mut)]
    pub student: Signer<'info>,

    #[account(
        seeds = [b"student", student.key().as_ref()],
        bump = student_profile.bump,
        constraint = student_profile.authority == student.key()
    )]
    pub student_profile: Account<'info, StudentProfile>,

    #[account(mut)]
    pub grant: Account<'info, Grant>,

    #[account(
        init,
        payer = student,
        space = DISCRIMINATOR_LENGTH + Milestone::INIT_SPACE,
        seeds = [b"milestone", grant.key().as_ref(), student_profile.key().as_ref(), &[milestone_index]],
        bump
    )]
    pub milestone: Account<'info, Milestone>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CastVouch<'info> {
    #[account(mut)]
    pub voter: Signer<'info>,

    #[account(
        mut,
        seeds = [b"student", voter.key().as_ref()],
        bump = voter_profile.bump,
        constraint = voter_profile.authority == voter.key()
    )]
    pub voter_profile: Account<'info, StudentProfile>,

    #[account(
        mut,
        seeds = [b"student", builder_profile.authority.as_ref()],
        bump = builder_profile.bump
    )]
    pub builder_profile: Account<'info, StudentProfile>,

    #[account(mut)]
    pub milestone: Account<'info, Milestone>,
}

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct ReleaseMilestone<'info> {
    #[account(
        mut,
        constraint = sponsor.key() == grant.authority @ CampusFlowError::UnauthorizedSponsor
    )]
    pub sponsor: Signer<'info>,

    /// CHECK: Target recipient verified through milestone record
    #[account(mut)]
    pub builder: AccountInfo<'info>,

    #[account(
        seeds = [b"student", builder.key().as_ref()],
        bump = builder_profile.bump,
        constraint = builder_profile.key() == milestone.builder @ CampusFlowError::InvalidBuilder
    )]
    pub builder_profile: Account<'info, StudentProfile>,

    #[account(
        mut,
        seeds = [b"grant", sponsor.key().as_ref(), &grant_id.to_le_bytes()],
        bump = grant.bump,
        constraint = grant.key() == milestone.grant
    )]
    pub grant: Account<'info, Grant>,

    #[account(
        mut,
        seeds = [b"milestone", grant.key().as_ref(), builder_profile.key().as_ref(), &[milestone.milestone_index]],
        bump = milestone.bump
    )]
    pub milestone: Account<'info, Milestone>,

    #[account(
        mut,
        seeds = [b"vault", grant.key().as_ref()],
        bump = grant.vault_bump,
        constraint = vault.key() == grant.vault
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = builder_token_account.owner == builder.key(),
        constraint = builder_token_account.mint == grant.mint
    )]
    pub builder_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

// -----------------------------------------------------------------------------
// DATA STATE SCHEMAS
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct Grant {
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub grant_id: u64,
    pub total_budget: u64,
    pub disbursed_amount: u64,
    #[max_len(64)]
    pub title: String,
    pub bump: u8,
    pub vault_bump: u8,
    pub created_at: i64,
}

#[account]
#[derive(InitSpace)]
pub struct StudentProfile {
    pub authority: Pubkey,
    pub institution_hash: [u8; 32],
    pub vouch_count: u32,
    pub reputation_score: u32,
    pub bump: u8,
    pub registered_at: i64,
}

#[account]
#[derive(InitSpace)]
pub struct Milestone {
    pub grant: Pubkey,
    pub builder: Pubkey,
    pub payout_amount: u64,
    pub vouch_count: u32,
    pub milestone_index: u8,
    pub is_completed: bool,
    pub is_disbursed: bool,
    #[max_len(128)]
    pub deliverable_uri: String,
    pub bump: u8,
    pub submitted_at: i64,
}

// -----------------------------------------------------------------------------
// EVENTS & ERRORS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantInitializedEvent {
    pub grant_id: u64,
    pub sponsor: Pubkey,
    pub total_budget: u64,
    pub timestamp: i64,
}

#[event]
pub struct StudentRegisteredEvent {
    pub student: Pubkey,
    pub institution_hash: [u8; 32],
    pub timestamp: i64,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub grant: Pubkey,
    pub builder: Pubkey,
    pub milestone_index: u8,
    pub payout_amount: u64,
}

#[event]
pub struct VouchCastEvent {
    pub milestone: Pubkey,
    pub voter: Pubkey,
    pub total_vouches: u32,
}

#[event]
pub struct MilestoneReleasedEvent {
    pub grant: Pubkey,
    pub milestone: Pubkey,
    pub builder: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[error_code]
pub enum CampusFlowError {
    #[msg("Title string exceeds maximum length")]
    TitleTooLong,
    #[msg("Invalid grant initial budget")]
    InvalidBudget,
    #[msg("Deliverable URI string exceeds maximum length")]
    UriTooLong,
    #[msg("Payout amount must be greater than zero")]
    InvalidAmount,
    #[msg("Milestone payout exceeds remaining grant balance")]
    InsufficientGrantFunds,
    #[msg("Arithmetic overflow detected")]
    MathOverflow,
    #[msg("Students cannot vouch for their own milestones")]
    SelfVouchingNotAllowed,
    #[msg("Milestone is already marked as completed")]
    MilestoneAlreadyCompleted,
    #[msg("Milestone funds have already been disbursed")]
    AlreadyDisbursed,
    #[msg("Insufficient peer vouches to meet release threshold")]
    NotEnoughVouches,
    #[msg("Signer is not the authorized sponsor")]
    UnauthorizedSponsor,
    #[msg("Provided builder does not match registered profile")]
    InvalidBuilder,
}