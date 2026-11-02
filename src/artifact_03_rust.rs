use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("V1beStream1111111111111111111111111111111111");

pub const MAX_NAME_LEN: usize = 64;
pub const BPS_DENOMINATOR: u16 = 10_000;

#[program]
pub mod vibestream {
    use super::*;

    /// Initialize a new campus event with an escrow vault.
    pub fn initialize_event(
        ctx: Context<InitializeEvent>,
        event_id: u64,
        name: String,
        ticket_price: u64,
        max_attendees: u32,
        treasury_split_bps: u16,
    ) -> Result<()> {
        require!(name.len() <= MAX_NAME_LEN, VibeError::NameTooLong);
        require!(treasury_split_bps <= 3000, VibeError::ExcessiveTreasuryCut); // Max 30% campus tax
        require!(max_attendees > 0, VibeError::ZeroCapacity);

        let event = &mut ctx.accounts.event_account;
        event.organizer = ctx.accounts.organizer.key();
        event.treasury = ctx.accounts.treasury.key();
        event.event_id = event_id;
        event.ticket_price = ticket_price;
        event.max_attendees = max_attendees;
        event.current_attendees = 0;
        event.checked_in_count = 0;
        event.treasury_split_bps = treasury_split_bps;
        event.is_active = true;
        event.bump = ctx.bumps.event_account;
        event.vault_bump = ctx.bumps.vault_account;
        event.name = name.clone();

        emit!(EventCreated {
            event_id,
            organizer: ctx.accounts.organizer.key(),
            ticket_price,
            max_attendees,
            name,
        });

        Ok(())
    }

    /// Attendee deposits ticket price into event PDA vault.
    pub fn register_attendee(ctx: Context<RegisterAttendee>, event_id: u64) -> Result<()> {
        let event = &mut ctx.accounts.event_account;
        require!(event.is_active, VibeError::EventInactive);
        require!(
            event.current_attendees < event.max_attendees,
            VibeError::EventSoldOut
        );

        // Lock funds into the deterministic vault PDA
        let cpi_accounts = Transfer {
            from: ctx.accounts.attendee.to_account_info(),
            to: ctx.accounts.vault_account.to_account_info(),
        };
        let cpi_program = ctx.accounts.system_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        transfer(cpi_ctx, event.ticket_price)?;

        let registration = &mut ctx.accounts.registration_account;
        registration.attendee = ctx.accounts.attendee.key();
        registration.event_id = event_id;
        registration.paid_amount = event.ticket_price;
        registration.is_checked_in = false;
        registration.registered_at = Clock::get()?.unix_timestamp;
        registration.bump = ctx.bumps.registration_account;

        event.current_attendees = event
            .current_attendees
            .checked_add(1)
            .ok_or(VibeError::MathOverflow)?;

        emit!(AttendeeRegistered {
            event_id,
            attendee: ctx.accounts.attendee.key(),
            paid_amount: event.ticket_price,
            timestamp: registration.registered_at,
        });

        Ok(())
    }

    /// Organizer checks in an attendee, triggering an immediate escrow disbursement.
    pub fn checkin_and_disburse(
        ctx: Context<CheckinAndDisburse>,
        _event_id: u64,
    ) -> Result<()> {
        let event = &mut ctx.accounts.event_account;
        require!(event.is_active, VibeError::EventInactive);

        let registration = &mut ctx.accounts.registration_account;
        require!(!registration.is_checked_in, VibeError::AlreadyCheckedIn);
        require!(
            registration.attendee == ctx.accounts.attendee.key(),
            VibeError::AttendeeMismatch
        );

        registration.is_checked_in = true;
        event.checked_in_count = event
            .checked_in_count
            .checked_add(1)
            .ok_or(VibeError::MathOverflow)?;

        let total_deposit = registration.paid_amount;

        if total_deposit > 0 {
            let treasury_cut = (total_deposit as u128)
                .checked_mul(event.treasury_split_bps as u128)
                .ok_or(VibeError::MathOverflow)?
                .checked_div(BPS_DENOMINATOR as u128)
                .ok_or(VibeError::MathOverflow)? as u64;

            let organizer_cut = total_deposit
                .checked_sub(treasury_cut)
                .ok_or(VibeError::MathOverflow)?;

            // Transfer from Vault PDA via PDA Signer seeds
            let event_key = event.key();
            let vault_seeds: &[&[&[u8]]] = &[&[
                b"vault",
                event_key.as_ref(),
                &[event.vault_bump],
            ]];

            if treasury_cut > 0 {
                let treasury_transfer = Transfer {
                    from: ctx.accounts.vault_account.to_account_info(),
                    to: ctx.accounts.treasury.to_account_info(),
                };
                let cpi_treasury = CpiContext::new_with_signer(
                    ctx.accounts.system_program.to_account_info(),
                    treasury_transfer,
                    vault_seeds,
                );
                transfer(cpi_treasury, treasury_cut)?;
            }

            let organizer_transfer = Transfer {
                from: ctx.accounts.vault_account.to_account_info(),
                to: ctx.accounts.organizer.to_account_info(),
            };
            let cpi_organizer = CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                organizer_transfer,
                vault_seeds,
            );
            transfer(cpi_organizer, organizer_cut)?;
        }

        emit!(AttendeeCheckedIn {
            event_id: event.event_id,
            attendee: ctx.accounts.attendee.key(),
            organizer_payout: total_deposit,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Emergency cancellation by organizer: refunds attendee registrations.
    pub fn refund_attendee(ctx: Context<RefundAttendee>, _event_id: u64) -> Result<()> {
        let event = &ctx.accounts.event_account;
        require!(!event.is_active, VibeError::EventMustBeCancelled);

        let registration = &mut ctx.accounts.registration_account;
        require!(!registration.is_checked_in, VibeError::AlreadyCheckedIn);
        require!(registration.paid_amount > 0, VibeError::ZeroBalance);

        let refund_amount = registration.paid_amount;
        registration.paid_amount = 0;

        let event_key = event.key();
        let vault_seeds: &[&[&[u8]]] = &[&[
            b"vault",
            event_key.as_ref(),
            &[event.vault_bump],
        ]];

        let refund_transfer = Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.attendee.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            refund_transfer,
            vault_seeds,
        );
        transfer(cpi_ctx, refund_amount)?;

        emit!(AttendeeRefunded {
            event_id: event.event_id,
            attendee: ctx.accounts.attendee.key(),
            amount: refund_amount,
        });

        Ok(())
    }

    /// Organizer halts the event, allowing remaining attendees to claim refunds.
    pub fn cancel_event(ctx: Context<CancelEvent>, _event_id: u64) -> Result<()> {
        let event = &mut ctx.accounts.event_account;
        event.is_active = false;
        emit!(EventCancelled {
            event_id: event.event_id
        });
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(event_id: u64)]
pub struct InitializeEvent<'info> {
    #[account(
        init,
        payer = organizer,
        space = 8 + EventAccount::INIT_SPACE,
        seeds = [b"event", organizer.key().as_ref(), &event_id.to_le_bytes()],
        bump
    )]
    pub event_account: Account<'info, EventAccount>,

    /// CHECK: Vault PDA derived deterministically from the event account
    #[account(
        mut,
        seeds = [b"vault", event_account.key().as_ref()],
        bump
    )]
    pub vault_account: SystemAccount<'info>,

    #[account(mut)]
    pub organizer: Signer<'info>,

    /// CHECK: Unchecked campus treasury key (e.g. Student Union / DAO multi-sig)
    pub treasury: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(event_id: u64)]
pub struct RegisterAttendee<'info> {
    #[account(
        mut,
        seeds = [b"event", event_account.organizer.as_ref(), &event_id.to_le_bytes()],
        bump = event_account.bump
    )]
    pub event_account: Account<'info, EventAccount>,

    #[account(
        init,
        payer = attendee,
        space = 8 + RegistrationAccount::INIT_SPACE,
        seeds = [b"registration", event_account.key().as_ref(), attendee.key().as_ref()],
        bump
    )]
    pub registration_account: Account<'info, RegistrationAccount>,

    /// CHECK: Validated vault receiving ticket funds
    #[account(
        mut,
        seeds = [b"vault", event_account.key().as_ref()],
        bump = event_account.vault_bump
    )]
    pub vault_account: SystemAccount<'info>,

    #[account(mut)]
    pub attendee: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(event_id: u64)]
pub struct CheckinAndDisburse<'info> {
    #[account(
        mut,
        seeds = [b"event", organizer.key().as_ref(), &event_id.to_le_bytes()],
        bump = event_account.bump,
        has_one = organizer @ VibeError::UnauthorizedOrganizer,
        has_one = treasury @ VibeError::InvalidTreasuryAccount
    )]
    pub event_account: Account<'info, EventAccount>,

    #[account(
        mut,
        seeds = [b"registration", event_account.key().as_ref(), attendee.key().as_ref()],
        bump = registration_account.bump
    )]
    pub registration_account: Account<'info, RegistrationAccount>,

    /// CHECK: Vault releasing payments
    #[account(
        mut,
        seeds = [b"vault", event_account.key().as_ref()],
        bump = event_account.vault_bump
    )]
    pub vault_account: SystemAccount<'info>,

    pub organizer: Signer<'info>,

    /// CHECK: Attendee identity being verified
    #[account(mut)]
    pub attendee: SystemAccount<'info>,

    /// CHECK: Event treasury receiving BPS cut
    #[account(mut)]
    pub treasury: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(event_id: u64)]
pub struct RefundAttendee<'info> {
    #[account(
        seeds = [b"event", event_account.organizer.as_ref(), &event_id.to_le_bytes()],
        bump = event_account.bump
    )]
    pub event_account: Account<'info, EventAccount>,

    #[account(
        mut,
        seeds = [b"registration", event_account.key().as_ref(), attendee.key().as_ref()],
        bump = registration_account.bump,
        has_one = attendee @ VibeError::AttendeeMismatch
    )]
    pub registration_account: Account<'info, RegistrationAccount>,

    /// CHECK: Vault releasing refund
    #[account(
        mut,
        seeds = [b"vault", event_account.key().as_ref()],
        bump = event_account.vault_bump
    )]
    pub vault_account: SystemAccount<'info>,

    #[account(mut)]
    pub attendee: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(event_id: u64)]
pub struct CancelEvent<'info> {
    #[account(
        mut,
        seeds = [b"event", organizer.key().as_ref(), &event_id.to_le_bytes()],
        bump = event_account.bump,
        has_one = organizer @ VibeError::UnauthorizedOrganizer
    )]
    pub event_account: Account<'info, EventAccount>,

    pub organizer: Signer<'info>,
}

// -----------------------------------------------------------------------------
// STATE SCHEMAS & EVENTS
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct EventAccount {
    pub organizer: Pubkey,
    pub treasury: Pubkey,
    pub event_id: u64,
    pub ticket_price: u64,
    pub max_attendees: u32,
    pub current_attendees: u32,
    pub checked_in_count: u32,
    pub treasury_split_bps: u16,
    pub is_active: bool,
    pub bump: u8,
    pub vault_bump: u8,
    #[max_len(64)]
    pub name: String,
}

#[account]
#[derive(InitSpace)]
pub struct RegistrationAccount {
    pub attendee: Pubkey,
    pub event_id: u64,
    pub paid_amount: u64,
    pub is_checked_in: bool,
    pub registered_at: i64,
    pub bump: u8,
}

#[event]
pub struct EventCreated {
    pub event_id: u64,
    pub organizer: Pubkey,
    pub ticket_price: u64,
    pub max_attendees: u32,
    pub name: String,
}

#[event]
pub struct AttendeeRegistered {
    pub event_id: u64,
    pub attendee: Pubkey,
    pub paid_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct AttendeeCheckedIn {
    pub event_id: u64,
    pub attendee: Pubkey,
    pub organizer_payout: u64,
    pub timestamp: i64,
}

#[event]
pub struct AttendeeRefunded {
    pub event_id: u64,
    pub attendee: Pubkey,
    pub amount: u64,
}

#[event]
pub struct EventCancelled {
    pub event_id: u64,
}

// -----------------------------------------------------------------------------
// ERROR DEFINITIONS
// -----------------------------------------------------------------------------

#[error_code]
pub enum VibeError {
    #[msg("Event name exceeds maximum length of 64 characters.")]
    NameTooLong,
    #[msg("Treasury split cannot exceed 3000 basis points (30%).")]
    ExcessiveTreasuryCut,
    #[msg("Capacity must be greater than zero.")]
    ZeroCapacity,
    #[msg("Event is not currently active.")]
    EventInactive,
    #[msg("Maximum attendee capacity reached.")]
    EventSoldOut,
    #[msg("Arithmetic overflow detected.")]
    MathOverflow,
    #[msg("Attendee is already checked in.")]
    AlreadyCheckedIn,
    #[msg("Attendee public key does not match registration record.")]
    AttendeeMismatch,
    #[msg("Signer is not authorized organizer for this event.")]
    UnauthorizedOrganizer,
    #[msg("Supplied treasury key does not match initialized configuration.")]
    InvalidTreasuryAccount,
    #[msg("Event must be cancelled before claiming refunds.")]
    EventMustBeCancelled,
    #[msg("Registration has no remaining paid balance to refund.")]
    ZeroBalance,
}