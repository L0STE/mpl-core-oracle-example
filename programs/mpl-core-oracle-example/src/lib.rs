use anchor_lang::{
    prelude::*, 
    system_program::{transfer, Transfer}
};
use mpl_core::types::{OracleValidation, ExternalValidationResult};

declare_id!("9FqtzMakHHBzhLvj2iTQDY7CtPM6yscvRjwr5aKhrakJ");

#[program]
pub mod mpl_core_oracle_example {
    use super::*;

    pub fn create_oracle(ctx: Context<CreateOracle>) -> Result<()> {
        // Set the Oracle validation based on the time and if the US market is open
        match is_us_market_open(Clock::get()?.unix_timestamp) {
            true => {
                ctx.accounts.oracle.set_inner(
                    Oracle {
                        validation: OracleValidation::V1 {
                            transfer: ExternalValidationResult::Approved,
                            create: ExternalValidationResult::Pass,
                            update: ExternalValidationResult::Pass,
                            burn: ExternalValidationResult::Pass,
                        },
                        bump: ctx.bumps.oracle,
                        vault_bump: ctx.bumps.reward_vault,
                    }
                );
            }
            false => {
                ctx.accounts.oracle.set_inner(
                    Oracle {
                        validation: OracleValidation::V1 {
                            transfer: ExternalValidationResult::Rejected,
                            create: ExternalValidationResult::Pass,
                            update: ExternalValidationResult::Pass,
                            burn: ExternalValidationResult::Pass,
                        },
                        bump: ctx.bumps.oracle,
                        vault_bump: ctx.bumps.reward_vault,
                    }
                );
            }
        }

        Ok(())
    }

    pub fn crank_oracle(ctx: Context<CrankOracle>) -> Result<()> {
        match is_us_market_open(Clock::get()?.unix_timestamp) {
            true => {
                require!(ctx.accounts.oracle.validation == OracleValidation::V1 {transfer: ExternalValidationResult::Rejected, create: ExternalValidationResult::Pass, burn: ExternalValidationResult::Pass, update: ExternalValidationResult::Pass }, Errors::AlreadyUpdated);
                ctx.accounts.oracle.validation = OracleValidation::V1 {
                    transfer: ExternalValidationResult::Approved,
                    create: ExternalValidationResult::Pass,
                    burn: ExternalValidationResult::Pass,
                    update: ExternalValidationResult::Pass,
                };
            }
            false => {
                require!(ctx.accounts.oracle.validation == OracleValidation::V1 { transfer: ExternalValidationResult::Approved, create: ExternalValidationResult::Pass, burn: ExternalValidationResult::Pass, update: ExternalValidationResult::Pass }, Errors::AlreadyUpdated);
                ctx.accounts.oracle.validation = OracleValidation::V1 {
                    transfer: ExternalValidationResult::Rejected,
                    create: ExternalValidationResult::Pass,
                    burn: ExternalValidationResult::Pass,
                    update: ExternalValidationResult::Pass,
                };
            }
        }

        let reward_vault_lamports = ctx.accounts.reward_vault.lamports();
        let oracle_key = ctx.accounts.oracle.key().clone();
        let signer_seeds = &[b"reward_vault", oracle_key.as_ref(), &[ctx.accounts.oracle.bump]];
        match is_within_15_minutes_of_market_open_or_close(Clock::get()?.unix_timestamp) && reward_vault_lamports > REWARD_IN_LAMPORTS {
            true => {
                // Reward cranker for updating Oracle within 15 minutes of market open or close
                transfer(
                    CpiContext::new_with_signer(
                        ctx.accounts.system_program.to_account_info(), 
                        Transfer {
                            from: ctx.accounts.reward_vault.to_account_info(),
                            to: ctx.accounts.signer.to_account_info(),
                        }, 
                        &[signer_seeds]
                    ),
                    REWARD_IN_LAMPORTS
                )?
            }
            false => {
                // Do nothing
            }
        }
        Ok(())
    }
}

// Accounts
#[derive(Accounts)]
pub struct CreateOracle<'info> {
    pub signer: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = Oracle::INIT_SPACE,
        seeds = [b"oracle"],
        bump
    )]
    pub oracle: Account<'info, Oracle>,
    #[account(
        seeds = [b"reward_vault", oracle.key().as_ref()],
        bump,
    )]
    pub reward_vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CrankOracle<'info> {
    pub signer: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"oracle"],
        bump = oracle.bump,
    )]
    pub oracle: Account<'info, Oracle>,
    #[account(
        mut, 
        seeds = [b"reward_vault", oracle.key().as_ref()],
        bump = oracle.vault_bump,
    )]
    pub reward_vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

// States
#[account]
pub struct Oracle {
    pub validation: OracleValidation,
    pub bump: u8,
    pub vault_bump: u8,
}

impl Space for Oracle {
    const INIT_SPACE: usize = 8 + 5 + 1;
}

// Errors
#[error_code]
pub enum Errors {
    #[msg("Oracle already updated")]
    AlreadyUpdated,
}

// Constants
const SECONDS_IN_AN_HOUR: i64 = 3600;
const SECONDS_IN_A_MINUTE: i64 = 60;
const SECONDS_IN_A_DAY: i64 = 86400;
const MARKET_OPEN_TIME: i64 = 14 * SECONDS_IN_AN_HOUR + 30 * SECONDS_IN_A_MINUTE; // 14:30 UTC == 9:30 EST
const MARKET_CLOSE_TIME: i64 = 21 * SECONDS_IN_AN_HOUR; // 21:00 UTC == 16:00 EST
const MARKET_OPEN_CLOSE_MARGIN: i64 = 15 * SECONDS_IN_A_MINUTE; // 15 minutes in seconds
const REWARD_IN_LAMPORTS: u64 = 10000000; // 0.001 SOL

// Helpers
fn is_us_market_open(unix_timestamp: i64) -> bool {
    let seconds_since_midnight = unix_timestamp % SECONDS_IN_A_DAY;
    let weekday = (unix_timestamp / SECONDS_IN_A_DAY + 4) % 7;

    // Check if it's a weekday (Monday = 0, ..., Friday = 4)
    if weekday >= 5 {
        return false;
    }

    // Check if current time is within market hours
    seconds_since_midnight >= MARKET_OPEN_TIME && seconds_since_midnight < MARKET_CLOSE_TIME
}

fn is_within_15_minutes_of_market_open_or_close(unix_timestamp: i64) -> bool {
    let seconds_since_midnight = unix_timestamp % SECONDS_IN_A_DAY;

    // Check if current time is within 15 minutes after market open or within 15 minutes after market close
    (seconds_since_midnight >= MARKET_OPEN_TIME && seconds_since_midnight < MARKET_OPEN_TIME + MARKET_OPEN_CLOSE_MARGIN) ||
    (seconds_since_midnight >= MARKET_CLOSE_TIME && seconds_since_midnight < MARKET_CLOSE_TIME + MARKET_OPEN_CLOSE_MARGIN)
}