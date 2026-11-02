use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("V1beS1ream111111111111111111111111111111111");

pub const MAX_TITLE_LEN: usize = 64;
pub const MAX_URI_LEN: usize = 128;
pub const MAX_MILESTONES: usize = 5;

#[program]
pub mod vibestream {
    use super::*;

    /// Initialize a new milestone-governed grant escrow
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        grant_id: u64,
        title: String,
        milestone_amounts: Vec<u64>,
        reviewer: Pubkey,
    ) -> Result<()> {
        require!(title.len() <= MAX_TITLE_LEN, VibeStreamError::TitleTooLong);
        require!(
            !milestone_amounts.is_empty() && milestone_amounts.len() <= MAX_MILESTONES,
            VibeStreamError::InvalidMilestoneCount
        );

        let total_grant_amount: u64 = milestone_amounts.iter().try_fold(0u64, |acc, &val| {
            acc.checked_add(val).ok_or(VibeStreamError::MathOverflow)
        })?;

        require!(total_grant_amount > 0, VibeStreamError::ZeroGrantAmount);

        let grant = &mut ctx.accounts.grant_state;
        grant.sponsor = ctx.accounts.sponsor.key();
        grant.recipient = ctx.accounts.recipient.key();
        grant.reviewer = reviewer;
        grant.mint = ctx.accounts.mint.key();
        grant.vault = ctx.accounts.vault.key();
        grant.grant_id = grant_id;
        grant.total_amount = total_grant_amount;
        grant.claimed_amount = 0;
        grant.current_milestone_index = 0;
        grant.is_active = true;
        grant.bump = ctx.bumps.grant_state;
        grant.vault_bump = ctx.bumps.vault;

        let mut milestones = Vec::new();
        for (i, amt) in milestone_amounts.iter().enumerate() {
            milestones.push(Milestone {
                index: i as u8,
                amount: *amt,
                status: MilestoneStatus::NotStarted,
                submission_uri: [0u8; MAX_URI_LEN],
                submission_uri_len: 0,
            });
        }
        grant.milestones = milestones;

        // Escrow funds from sponsor token account to vault PDA
        let cpi_accounts = Transfer {
            from: ctx.accounts.sponsor_token.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.sponsor.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), total_grant_amount)?;

        emit!(GrantInitializedEvent {
            grant_id,
            sponsor: ctx.accounts.sponsor.key(),
            recipient: ctx.accounts.recipient.key(),
            reviewer,
            total_amount: total_grant_amount,
            milestones_count: grant.milestones.len() as u8,
        });

        Ok(())
    }

    /// Builder submits milestone proof (e.g. GitHub commit or IPFS deliverable hash)
    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        grant_id: u64,
        milestone_index: u8,
        submission_uri: String,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_state;
        require!(grant.is_active, VibeStreamError::GrantNotActive);
        require!(
            (milestone_index as usize) < grant.milestones.len(),
            VibeStreamError::InvalidMilestoneIndex
        );
        require!(
            milestone_index == grant.current_milestone_index,
            VibeStreamError::MilestoneOutOfOrder
        );

        let uri_bytes = submission_uri.as_bytes();
        require!(uri_bytes.len() <= MAX_URI_LEN, VibeStreamError::UriTooLong);

        let milestone = &mut grant.milestones[milestone_index as usize];
        require!(
            milestone.status == MilestoneStatus::NotStarted || milestone.status == MilestoneStatus::Rejected,
            VibeStreamError::MilestoneAlreadySubmitted
        );

        let mut fixed_uri = [0u8; MAX_URI_LEN];
        fixed_uri[..uri_bytes.len()].copy_from_slice(uri_bytes);

        milestone.submission_uri = fixed_uri;
        milestone.submission_uri_len = uri_bytes.len() as u8;
        milestone.status = MilestoneStatus::Submitted;

        emit!(MilestoneSubmittedEvent {
            grant_id,
            milestone_index,
            submission_uri,
            submitted_by: ctx.accounts.recipient.key(),
        });

        Ok(())
    }

    /// Designated Reviewer or Sponsor verifies and unlocks milestone capital
    pub fn review_milestone(
        ctx: Context<ReviewMilestone>,
        grant_id: u64,
        milestone_index: u8,
        approved: bool,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_state;
        require!(grant.is_active, VibeStreamError::GrantNotActive);
        require!(
            (milestone_index as usize) < grant.milestones.len(),
            VibeStreamError::InvalidMilestoneIndex
        );
        require!(
            milestone_index == grant.current_milestone_index,
            VibeStreamError::MilestoneOutOfOrder
        );

        let milestone = &mut grant.milestones[milestone_index as usize];
        require!(
            milestone.status == MilestoneStatus::Submitted,
            VibeStreamError::MilestoneNotSubmitted
        );

        if approved {
            milestone.status = MilestoneStatus::Approved;
            grant.current_milestone_index = grant.current_milestone_index.saturating_add(1);
        } else {
            milestone.status = MilestoneStatus::Rejected;
        }

        emit!(MilestoneReviewedEvent {
            grant_id,
            milestone_index,
            approved,
            reviewer: ctx.accounts.reviewer.key(),
        });

        Ok(())
    }

    /// Builder claims approved funds from escrow vault
    pub fn claim_funds(ctx: Context<ClaimFunds>, grant_id: u64) -> Result<()> {
        let grant = &mut ctx.accounts.grant_state;
        require!(grant.is_active, VibeStreamError::GrantNotActive);

        // Calculate total approved funds
        let mut approved_amount: u64 = 0;
        for ms in &grant.milestones {
            if ms.status == MilestoneStatus::Approved {
                approved_amount = approved_amount
                    .checked_add(ms.amount)
                    .ok_or(VibeStreamError::MathOverflow)?;
            }
        }

        let claimable = approved_amount
            .checked_sub(grant.claimed_amount)
            .ok_or(VibeStreamError::MathOverflow)?;

        require!(claimable > 0, VibeStreamError::NoFundsAvailable);

        grant.claimed_amount = grant
            .claimed_amount
            .checked_add(claimable)
            .ok_or(VibeStreamError::MathOverflow)?;

        // Execute CPI transfer from PDA Vault to Builder ATA using PDA seeds
        let sponsor_key = grant.sponsor;
        let grant_id_bytes = grant_id.to_le_bytes();
        let seeds = &[
            b"vault".as_ref(),
            sponsor_key.as_ref(),
            grant_id_bytes.as_ref(),
            &[grant.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.recipient_token.to_account_info(),
            authority: ctx.accounts.vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(
            CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds),
            claimable,
        )?;

        emit!(FundsClaimedEvent {
            grant_id,
            recipient: ctx.accounts.recipient.key(),
            amount: claimable,
            total_claimed: grant.claimed_amount,
        });

        Ok(())
    }

    /// Sponsor can claw back unvested/unapproved funds if project stalls
    pub fn cancel_and_clawback(ctx: Context<CancelAndClawback>, grant_id: u64) -> Result<()> {
        let grant = &mut ctx.accounts.grant_state;
        require!(grant.is_active, VibeStreamError::GrantNotActive);

        let vault_balance = ctx.accounts.vault.amount;
        require!(vault_balance > 0, VibeStreamError::ZeroGrantAmount);

        grant.is_active = false;

        let sponsor_key = grant.sponsor;
        let grant_id_bytes = grant_id.to_le_bytes();
        let seeds = &[
            b"vault".as_ref(),
            sponsor_key.as_ref(),
            grant_id_bytes.as_ref(),
            &[grant.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.sponsor_token.to_account_info(),
            authority: ctx.accounts.vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(
            CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds),
            vault_balance,
        )?;

        emit!(GrantCancelledEvent {
            grant_id,
            sponsor: grant.sponsor,
            reclaimed_amount: vault_balance,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub征struct InitializeGrant<'info> {
    #[account(
        init,
        payer = sponsor,
        space = GrantState::MAX_SIZE,
        seeds = [b"grant", sponsor.key().as_ref(), &grant_id.to_le_bytes()],
        bump
    )]
    pub grant_state: Account<'info, GrantState>,

    #[account(
        init,
        payer = sponsor,
        seeds = [b"vault", sponsor.key().as_ref(), &grant_id.to_le_bytes()],
        bump,
        token::mint = mint,
        token::authority = vault
    )]
    pub vault: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = sponsor_token.mint == mint.key() @ VibeStreamError::InvalidMint,
        constraint = sponsor_token.owner == sponsor.key() @ VibeStreamError::InvalidOwner
    )]
    pub sponsor_token: Account<'info, TokenAccount>,

    /// CHECK: Validated as recipient of future fund transfers
    pub recipient: AccountInfo<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct SubmitMilestone<'info> {
    #[account(
        mut,
        seeds = [b"grant", grant_state.sponsor.as_ref(), &grant_id.to_le_bytes()],
        bump = grant_state.bump,
        has_one = recipient @ VibeStreamError::UnauthorizedRecipient
    )]
    pub grant_state: Account<'info, GrantState>,

    pub recipient: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct ReviewMilestone<'info> {
    #[account(
        mut,
        seeds = [b"grant", grant_state.sponsor.as_ref(), &grant_id.to_le_bytes()],
        bump = grant_state.bump,
        constraint = (grant_state.reviewer == reviewer.key() || grant_state.sponsor == reviewer.key()) @ VibeStreamError::UnauthorizedReviewer
    )]
    pub grant_state: Account<'info, GrantState>,

    pub reviewer: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct ClaimFunds<'info> {
    #[account(
        mut,
        seeds = [b"grant", grant_state.sponsor.as_ref(), &grant_id.to_le_bytes()],
        bump = grant_state.bump,
        has_one = recipient @ VibeStreamError::UnauthorizedRecipient,
        has_one = vault @ VibeStreamError::InvalidVault
    )]
    pub grant_state: Account<'info, GrantState>,

    #[account(
        mut,
        seeds = [b"vault", grant_state.sponsor.as_ref(), &grant_id.to_le_bytes()],
        bump = grant_state.vault_bump
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = recipient_token.mint == grant_state.mint @ VibeStreamError::InvalidMint,
        constraint = recipient_token.owner == recipient.key() @ VibeStreamError::InvalidOwner
    )]
    pub recipient_token: Account<'info, TokenAccount>,

    pub recipient: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct CancelAndClawback<'info> {
    #[account(
        mut,
        seeds = [b"grant", sponsor.key().as_ref(), &grant_id.to_le_bytes()],
        bump = grant_state.bump,
        has_one = sponsor @ VibeStreamError::UnauthorizedSponsor,
        has_one = vault @ VibeStreamError::InvalidVault
    )]
    pub grant_state: Account<'info, GrantState>,

    #[account(
        mut,
        seeds = [b"vault", sponsor.key().as_ref(), &grant_id.to_le_bytes()],
        bump = grant_state.vault_bump
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = sponsor_token.mint == grant_state.mint @ VibeStreamError::InvalidMint,
        constraint = sponsor_token.owner == sponsor.key() @ VibeStreamError::InvalidOwner
    )]
    pub sponsor_token: Account<'info, TokenAccount>,

    pub sponsor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// -----------------------------------------------------------------------------
// STATE ENTITIES & TYPES
// -----------------------------------------------------------------------------

#[account]
pub struct GrantState {
    pub sponsor: Pubkey,
    pub recipient: Pubkey,
    pub reviewer: Pubkey,
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub grant_id: u64,
    pub total_amount: u64,
    pub claimed_amount: u64,
    pub current_milestone_index: u8,
    pub is_active: bool,
    pub bump: u8,
    pub vault_bump: u8,
    pub milestones: Vec<Milestone>,
}

impl GrantState {
    pub const MAX_SIZE: usize = 8 + // Discriminator
        32 + // sponsor
        32 + // recipient
        32 + // reviewer
        32 + // mint
        32 + // vault
        8  + // grant_id
        8  + // total_amount
        8  + // claimed_amount
        1  + // current_milestone_index
        1  + // is_active
        1  + // bump
        1  + // vault_bump
        4  + (MAX_MILESTONES * Milestone::SIZE); // Vec overhead + items
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum MilestoneStatus {
    NotStarted,
    Submitted,
    Approved,
    Rejected,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Milestone {
    pub index: u8,
    pub amount: u64,
    pub status: MilestoneStatus,
    pub submission_uri: [u8; MAX_URI_LEN],
    pub submission_uri_len: u8,
}

impl Milestone {
    pub const SIZE: usize = 1 + 8 + 1 + MAX_URI_LEN + 1;
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantInitializedEvent {
    pub grant_id: u64,
    pub sponsor: Pubkey,
    pub recipient: Pubkey,
    pub reviewer: Pubkey,
    pub total_amount: u64,
    pub milestones_count: u8,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub grant_id: u64,
    pub milestone_index: u8,
    pub submission_uri: String,
    pub submitted_by: Pubkey,
}

#[event]
pub struct MilestoneReviewedEvent {
    pub grant_id: u64,
    pub milestone_index: u8,
    pub approved: bool,
    pub reviewer: Pubkey,
}

#[event]
pub struct FundsClaimedEvent {
    pub grant_id: u64,
    pub recipient: Pubkey,
    pub amount: u64,
    pub total_claimed: u64,
}

#[event]
pub struct GrantCancelledEvent {
    pub grant_id: u64,
    pub sponsor: Pubkey,
    pub reclaimed_amount: u64,
}

// -----------------------------------------------------------------------------
// ERROR CODES
// -----------------------------------------------------------------------------

#[error_code]
pub enum VibeStreamError {
    #[msg("Title exceeds the maximum allowed length of 64 characters.")]
    TitleTooLong,
    #[msg("Milestone count must be between 1 and 5.")]
    InvalidMilestoneCount,
    #[msg("Total grant funding amount must be strictly greater than zero.")]
    ZeroGrantAmount,
    #[msg("Milestone index out of bounds.")]
    InvalidMilestoneIndex,
    #[msg("Milestones must be submitted and approved strictly in sequential order.")]
    MilestoneOutOfOrder,
    #[msg("Deliverable submission URI exceeds maximum length of 128 bytes.")]
    UriTooLong,
    #[msg("Milestone deliverable has already been submitted or approved.")]
    MilestoneAlreadySubmitted,
    #[msg("Milestone deliverable must be submitted before review.")]
    MilestoneNotSubmitted,
    #[msg("No unlocked funds currently available for claim.")]
    NoFundsAvailable,
    #[msg("The requested grant is closed, completed, or cancelled.")]
    GrantNotActive,
    #[msg("Caller is not authorized as the grant recipient.")]
    UnauthorizedRecipient,
    #[msg("Caller is not authorized to review milestones for this grant.")]
    UnauthorizedReviewer,
    #[msg("Caller is not authorized as the grant sponsor.")]
    UnauthorizedSponsor,
    #[msg("Mismatched token mint.")]
    InvalidMint,
    #[msg("Token account owner mismatch.")]
    InvalidOwner,
    #[msg("Vault PDA account verification mismatch.")]
    InvalidVault,
    #[msg("Arithmetic overflow detected.")]
    MathOverflow,
}