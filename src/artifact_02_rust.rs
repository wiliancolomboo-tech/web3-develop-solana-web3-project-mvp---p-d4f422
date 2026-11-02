use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("V1be7d2MKMz3B8e1Vw9N4Kz8C6uV8hR3XjB2pC4jA9Z");

pub const MAX_NAME_LEN: usize = 64;
pub const MAX_DESC_LEN: usize = 256;
pub const MAX_MILESTONES: usize = 5;

#[program]
pub mod univibe_core {
    use super::*;

    /// Initialize global protocol state with fee basis points (e.g. 50 = 0.5% for campus treasury)
    pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        fee_basis_points: u16,
    ) -> Result<()> {
        require!(fee_basis_points <= 1000, UniVibeError::FeeTooHigh); // Max 10%
        let config = &mut ctx.accounts.config;
        config.admin = ctx.accounts.admin.key();
        config.treasury = ctx.accounts.treasury.key();
        config.fee_basis_points = fee_basis_points;
        config.total_campaigns = 0;
        config.bump = ctx.bumps.config;

        msg!("UniVibe Protocol initialized successfully. Admin: {:?}", config.admin);
        Ok(())
    }

    /// Create a new campus campaign with predetermined milestone allocations
    pub fn create_campaign(
        ctx: Context<CreateCampaign>,
        campaign_id: u64,
        name: String,
        description: String,
        target_amount: u64,
        milestone_percentages: Vec<u8>,
    ) -> Result<()> {
        require!(name.len() <= MAX_NAME_LEN, UniVibeError::StringTooLong);
        require!(description.len() <= MAX_DESC_LEN, UniVibeError::StringTooLong);
        require!(target_amount > 0, UniVibeError::InvalidTargetAmount);
        require!(
            !milestone_percentages.is_empty() && milestone_percentages.len() <= MAX_MILESTONES,
            UniVibeError::InvalidMilestoneCount
        );

        let sum: u16 = milestone_percentages.iter().map(|&x| x as u16).sum();
        require!(sum == 100, UniVibeError::MilestonesMustSumTo100);

        let campaign = &mut ctx.accounts.campaign;
        campaign.creator = ctx.accounts.creator.key();
        campaign.campaign_id = campaign_id;
        campaign.name = name.clone();
        campaign.description = description;
        campaign.target_amount = target_amount;
        campaign.collected_amount = 0;
        campaign.withdrawn_amount = 0;
        campaign.milestone_count = milestone_percentages.len() as u8;
        campaign.milestone_percentages = milestone_percentages;
        campaign.current_milestone = 0;
        campaign.is_completed = false;
        campaign.vault_bump = ctx.bumps.vault;
        campaign.campaign_bump = ctx.bumps.campaign;

        let config = &mut ctx.accounts.config;
        config.total_campaigns = config.total_campaigns.checked_add(1).unwrap();

        emit!(CampaignCreatedEvent {
            campaign_id,
            campaign_pubkey: campaign.key(),
            creator: campaign.creator,
            target_amount,
            name,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Donate SOL into a campaign escrow vault
    pub fn donate(ctx: Context<Donate>, amount: u64) -> Result<()> {
        require!(amount > 0, UniVibeError::InvalidDonationAmount);
        require!(!ctx.accounts.campaign.is_completed, UniVibeError::CampaignFinished);

        // Transfer SOL from donor to campaign vault PDA
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.donor.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        );
        transfer(cpi_context, amount)?;

        let campaign = &mut ctx.accounts.campaign;
        campaign.collected_amount = campaign
            .collected_amount
            .checked_add(amount)
            .ok_or(UniVibeError::MathOverflow)?;

        emit!(DonationReceivedEvent {
            campaign_pubkey: campaign.key(),
            campaign_id: campaign.campaign_id,
            donor: ctx.accounts.donor.key(),
            amount,
            total_collected: campaign.collected_amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Unlock and disburse funds for the current validated milestone
    pub fn release_milestone(ctx: Context<ReleaseMilestone>) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;
        let config = &ctx.accounts.config;

        require!(!campaign.is_completed, UniVibeError::CampaignFinished);
        require!(
            (campaign.current_milestone as usize) < (campaign.milestone_count as usize),
            UniVibeError::AllMilestonesUnlocked
        );

        let percentage = campaign.milestone_percentages[campaign.current_milestone as usize];
        let gross_milestone_funds = (campaign.collected_amount as u128)
            .checked_mul(percentage as u128)
            .ok_or(UniVibeError::MathOverflow)?
            .checked_div(100)
            .ok_or(UniVibeError::MathOverflow)? as u64;

        require!(gross_milestone_funds > 0, UniVibeError::ZeroFundsToRelease);

        // Calculate protocol fee
        let fee_amount = (gross_milestone_funds as u128)
            .checked_mul(config.fee_basis_points as u128)
            .ok_or(UniVibeError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(UniVibeError::MathOverflow)? as u64;

        let net_payout = gross_milestone_funds
            .checked_sub(fee_amount)
            .ok_or(UniVibeError::MathOverflow)?;

        let campaign_id_bytes = campaign.campaign_id.to_le_bytes();
        let creator_key = campaign.creator.key();
        let seeds = &[
            b"vault",
            creator_key.as_ref(),
            campaign_id_bytes.as_ref(),
            &[campaign.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        // Transfer Protocol Fee to Treasury
        if fee_amount > 0 {
            let fee_cpi = CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.treasury.to_account_info(),
                },
                signer_seeds,
            );
            transfer(fee_cpi, fee_amount)?;
        }

        // Transfer Net Payout to Creator
        let payout_cpi = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.creator.to_account_info(),
            },
            signer_seeds,
        );
        transfer(payout_cpi, net_payout)?;

        campaign.withdrawn_amount = campaign
            .withdrawn_amount
            .checked_add(gross_milestone_funds)
            .ok_or(UniVibeError::MathOverflow)?;

        let released_milestone_index = campaign.current_milestone;
        campaign.current_milestone += 1;

        if campaign.current_milestone >= campaign.milestone_count {
            campaign.is_completed = true;
        }

        emit!(MilestoneReleasedEvent {
            campaign_pubkey: campaign.key(),
            campaign_id: campaign.campaign_id,
            milestone_index: released_milestone_index,
            amount_released: net_payout,
            fee_deducted: fee_amount,
            next_milestone: campaign.current_milestone,
            is_completed: campaign.is_completed,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT CONTEXTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + ProtocolConfig::LEN,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut)]
    pub admin: Signer<'info>,
    /// CHECK: Protocol Treasury address validated in instruction
    pub treasury: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(campaign_id: u64)]
pub struct CreateCampaign<'info> {
    #[account(
        init,
        payer = creator,
        space = 8 + CampaignAccount::LEN,
        seeds = [b"campaign", creator.key().as_ref(), &campaign_id.to_le_bytes()],
        bump
    )]
    pub campaign: Account<'info, CampaignAccount>,
    #[account(
        mut,
        seeds = [b"vault", creator.key().as_ref(), &campaign_id.to_le_bytes()],
        bump
    )]
    /// CHECK: Vault PDA holding native SOL
    pub vault: AccountInfo<'info>,
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Donate<'info> {
    #[account(
        mut,
        seeds = [b"campaign", campaign.creator.as_ref(), &campaign.campaign_id.to_le_bytes()],
        bump = campaign.campaign_bump
    )]
    pub campaign: Account<'info, CampaignAccount>,
    #[account(
        mut,
        seeds = [b"vault", campaign.creator.as_ref(), &campaign.campaign_id.to_le_bytes()],
        bump = campaign.vault_bump
    )]
    /// CHECK: Verified via PDA seed constraint
    pub vault: AccountInfo<'info>,
    #[account(mut)]
    pub donor: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReleaseMilestone<'info> {
    #[account(
        mut,
        seeds = [b"campaign", campaign.creator.as_ref(), &campaign.campaign_id.to_le_bytes()],
        bump = campaign.campaign_bump,
        has_one = creator @ UniVibeError::UnauthorizedCreator
    )]
    pub campaign: Account<'info, CampaignAccount>,
    #[account(
        mut,
        seeds = [b"vault", campaign.creator.as_ref(), &campaign.campaign_id.to_le_bytes()],
        bump = campaign.vault_bump
    )]
    /// CHECK: Vault PDA verified via PDA seed derivation
    pub vault: AccountInfo<'info>,
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut, address = config.treasury @ UniVibeError::InvalidTreasury)]
    /// CHECK: Checked against config treasury
    pub treasury: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// DATA STORAGE
// -----------------------------------------------------------------------------

#[account]
pub struct ProtocolConfig {
    pub admin: Pubkey,
    pub treasury: Pubkey,
    pub fee_basis_points: u16,
    pub total_campaigns: u64,
    pub bump: u8,
}

impl ProtocolConfig {
    pub const LEN: usize = 32 + 32 + 2 + 8 + 1;
}

#[account]
pub struct CampaignAccount {
    pub creator: Pubkey,
    pub campaign_id: u64,
    pub name: String,
    pub description: String,
    pub target_amount: u64,
    pub collected_amount: u64,
    pub withdrawn_amount: u64,
    pub milestone_count: u8,
    pub milestone_percentages: Vec<u8>,
    pub current_milestone: u8,
    pub is_completed: bool,
    pub vault_bump: u8,
    pub campaign_bump: u8,
}

impl CampaignAccount {
    pub const LEN: usize = 32
        + 8
        + (4 + MAX_NAME_LEN)
        + (4 + MAX_DESC_LEN)
        + 8
        + 8
        + 8
        + 1
        + (4 + MAX_MILESTONES)
        + 1
        + 1
        + 1
        + 1;
}

// -----------------------------------------------------------------------------
// EVENTS & ERRORS
// -----------------------------------------------------------------------------

#[event]
pub struct CampaignCreatedEvent {
    pub campaign_id: u64,
    pub campaign_pubkey: Pubkey,
    pub creator: Pubkey,
    pub target_amount: u64,
    pub name: String,
    pub timestamp: i64,
}

#[event]
pub struct DonationReceivedEvent {
    pub campaign_pubkey: Pubkey,
    pub campaign_id: u64,
    pub donor: Pubkey,
    pub amount: u64,
    pub total_collected: u64,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneReleasedEvent {
    pub campaign_pubkey: Pubkey,
    pub campaign_id: u64,
    pub milestone_index: u8,
    pub amount_released: u64,
    pub fee_deducted: u64,
    pub next_milestone: u8,
    pub is_completed: bool,
    pub timestamp: i64,
}

#[error_code]
pub enum UniVibeError {
    #[msg("Fee basis points exceed maximum permitted (1000 = 10%)")]
    FeeTooHigh,
    #[msg("Input string exceeds maximum allowed characters")]
    StringTooLong,
    #[msg("Target amount must be greater than zero")]
    InvalidTargetAmount,
    #[msg("Invalid milestone count. Must be between 1 and 5")]
    InvalidMilestoneCount,
    #[msg("Milestone percentages must sum up exactly to 100")]
    MilestonesMustSumTo100,
    #[msg("Donation amount must be greater than zero")]
    InvalidDonationAmount,
    #[msg("Campaign has already completed or finished")]
    CampaignFinished,
    #[msg("Arithmetic overflow occurred")]
    MathOverflow,
    #[msg("All milestones have already been unlocked and disbursed")]
    AllMilestonesUnlocked,
    #[msg("Zero funds are available for release in this milestone")]
    ZeroFundsToRelease,
    #[msg("Caller is not the authorized creator of this campaign")]
    UnauthorizedCreator,
    #[msg("Provided treasury does not match protocol configuration")]
    InvalidTreasury,
}