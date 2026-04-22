// SECURITY: Detailed architecture available in /SECURITY_DOSSIER.md
// Anchor 0.32.1 · MagicBlock Ephemeral Rollups

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::Instruction, program::invoke_signed};

declare_id!("5F8R6GdfgdkrQNPv5TTDEkcUw5Vtpy6irwWciLCBFCRp"); // ← replace with `anchor keys list`

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// MagicBlock Delegation Program (devnet + mainnet-beta)
pub const DELEGATION_PROGRAM_ID: Pubkey =
    pubkey!("DELeGGvXpWV2fqJUhqcF5ZSYMS4JTLjteaAMARRSaeSh");

/// Basis-point denominator (10_000 = 100%)
const BPS_DENOMINATOR: u64 = 10_000;

/// Annual yield rate expressed in basis points (e.g. 800 = 8% APY)
const ANNUAL_YIELD_BPS: u64 = 800;

/// Approximate number of Solana slots per year
/// (≈ 400 ms/slot × 2_628_000 slots/year)
const SLOTS_PER_YEAR: u64 = 78_840_000;

/// MagicBlock delegation instruction discriminator (Borsh, v0.4+)
const DELEGATE_DISCRIMINATOR:   [u8; 8] = [90, 147, 75, 178, 85, 88, 4, 137];
/// MagicBlock undelegation instruction discriminator
const UNDELEGATE_DISCRIMINATOR: [u8; 8] = [131, 148, 180, 198, 91, 104, 42, 12];

// ─────────────────────────────────────────────────────────────────────────────
// Program
// ─────────────────────────────────────────────────────────────────────────────

#[program]
pub mod shielded_yield {
    use super::*;

    // ── initialize ────────────────────────────────────────────────────────────
    /// Creates the Vault PDA for a given owner + numeric seed.
    /// One wallet can hold multiple vaults by varying `seed`.
    pub fn initialize(ctx: Context<Initialize>, seed: u64) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        let clock = Clock::get()?;

        vault.owner             = ctx.accounts.owner.key();
        vault.seed              = seed;
        vault.deposited         = 0;
        vault.yield_accrued     = 0;
        vault.delegated         = false;
        vault.deposit_slot      = clock.slot;
        vault.last_yield_slot   = clock.slot;
        vault.last_commit_slot  = 0;
        vault.bump              = ctx.bumps.vault;

        emit!(VaultInitialized {
            vault:  ctx.accounts.vault.key(),
            owner:  ctx.accounts.owner.key(),
            seed,
            slot:   clock.slot,
        });

        Ok(())
    }

    // ── deposit ───────────────────────────────────────────────────────────────
    /// Transfers lamports from the owner into the vault and records
    /// the deposit slot for yield-accrual bookkeeping.
    ///
    /// Can be called whether or not the vault is currently delegated.
    /// When delegated, this instruction should be sent to the *rollup* RPC
    /// so the ephemeral validator sees the updated balance immediately.
    pub fn deposit(ctx: Context<Deposit>, amount_lamports: u64) -> Result<()> {
        require!(amount_lamports > 0, VaultError::ZeroAmount);

        // Settle any pending yield before changing the principal
        accrue_yield(&mut ctx.accounts.vault)?;

        // Transfer lamports: owner → vault PDA
        let cpi_ctx = anchor_lang::system_program::Transfer {
            from: ctx.accounts.owner.to_account_info(),
            to:   ctx.accounts.vault.to_account_info(),
        };
        anchor_lang::system_program::transfer(
            CpiContext::new(ctx.accounts.system_program.to_account_info(), cpi_ctx),
            amount_lamports,
        )?;

        let vault = &mut ctx.accounts.vault;
        vault.deposited    = vault.deposited.checked_add(amount_lamports)
            .ok_or(VaultError::Overflow)?;
        vault.deposit_slot = Clock::get()?.slot;

        emit!(Deposited {
            vault:   vault.key(),
            amount:  amount_lamports,
            total:   vault.deposited,
            slot:    vault.deposit_slot,
        });

        Ok(())
    }

    // ── withdraw ──────────────────────────────────────────────────────────────
    /// Withdraws principal + all accrued yield to the owner.
    /// The vault must NOT be delegated at withdrawal time — call
    /// `undelegate` first so the latest rollup state is committed.
    pub fn withdraw(ctx: Context<Withdraw>, amount_lamports: u64) -> Result<()> {
        require!(!ctx.accounts.vault.delegated, VaultError::StillDelegated);
        require!(amount_lamports > 0, VaultError::ZeroAmount);

        // Settle yield before computing available balance
        accrue_yield(&mut ctx.accounts.vault)?;

        let vault = &ctx.accounts.vault;
        let available = vault.deposited
            .checked_add(vault.yield_accrued)
            .ok_or(VaultError::Overflow)?;
        require!(amount_lamports <= available, VaultError::InsufficientFunds);

        // PDA signer seeds
        let owner_key = vault.owner;
        let seed_bytes = vault.seed.to_le_bytes();
        let bump       = vault.bump;
        let seeds: &[&[&[u8]]] = &[&[b"vault", owner_key.as_ref(), &seed_bytes, &[bump]]];

        // Transfer lamports: vault PDA → owner
        let cpi_ctx = anchor_lang::system_program::Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to:   ctx.accounts.owner.to_account_info(),
        };
        anchor_lang::system_program::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                cpi_ctx,
                seeds,
            ),
            amount_lamports,
        )?;

        // Deduct from yield first, then principal
        let vault = &mut ctx.accounts.vault;
        let from_yield = amount_lamports.min(vault.yield_accrued);
        vault.yield_accrued = vault.yield_accrued.saturating_sub(from_yield);
        let from_principal  = amount_lamports.saturating_sub(from_yield);
        vault.deposited     = vault.deposited.saturating_sub(from_principal);

        emit!(Withdrawn {
            vault:    vault.key(),
            amount:   amount_lamports,
            slot:     Clock::get()?.slot,
        });

        Ok(())
    }

    // ── delegate ──────────────────────────────────────────────────────────────
    /// JIT-delegates the Vault PDA to the MagicBlock ephemeral rollup.
    ///
    /// `valid_until_slot`  — the rollup will reject transactions after this slot.
    ///                       Set to `u64::MAX` for no deadline.
    /// `commit_frequency`  — how often (milliseconds) the rollup commits
    ///                       state diffs back to base-layer. 5_000 = 5 s.
    pub fn delegate(
        ctx: Context<Delegate>,
        valid_until_slot:    u64,
        commit_frequency_ms: u32,
    ) -> Result<()> {
        require!(!ctx.accounts.vault.delegated, VaultError::AlreadyDelegated);

        // Settle any pending yield so the rollup starts with a clean slate
        accrue_yield(&mut ctx.accounts.vault)?;

        let vault     = &ctx.accounts.vault;
        let owner_key = ctx.accounts.owner.key();
        let seeds: &[&[&[u8]]] = &[&[
            b"vault",
            owner_key.as_ref(),
            &vault.seed.to_le_bytes(),
            &[vault.bump],
        ]];

        // Build CPI data: discriminator | valid_until_slot | commit_frequency_ms
        let mut data = Vec::with_capacity(20);
        data.extend_from_slice(&DELEGATE_DISCRIMINATOR);
        data.extend_from_slice(&valid_until_slot.to_le_bytes());
        data.extend_from_slice(&commit_frequency_ms.to_le_bytes());

        let ix = Instruction {
            program_id: DELEGATION_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(ctx.accounts.owner.key(),             true),  // payer
                AccountMeta::new(ctx.accounts.vault.key(),             true),  // delegated account
                AccountMeta::new_readonly(crate::ID,                   false), // owning program
                AccountMeta::new(ctx.accounts.delegation_buffer.key(), false), // state buffer
                AccountMeta::new_readonly(DELEGATION_PROGRAM_ID,       false), // delegation program
                AccountMeta::new_readonly(anchor_lang::solana_program::system_program::ID, false),
            ],
            data,
        };

        invoke_signed(
            &ix,
            &[
                ctx.accounts.owner.to_account_info(),
                ctx.accounts.vault.to_account_info(),
                ctx.accounts.delegation_program.to_account_info(),
                ctx.accounts.delegation_buffer.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            seeds,
        )?;

        let vault = &mut ctx.accounts.vault;
        vault.delegated         = true;
        vault.last_commit_slot  = Clock::get()?.slot;

        emit!(VaultDelegated {
            vault:            vault.key(),
            valid_until_slot,
            slot:             vault.last_commit_slot,
        });

        Ok(())
    }

    // ── undelegate ────────────────────────────────────────────────────────────
    /// Commits the latest ephemeral rollup state back to base-layer Solana
    /// and returns the Vault PDA to this program's ownership.
    ///
    /// ⚠  Send this transaction to a BASE-LAYER RPC, NOT the rollup endpoint.
    pub fn undelegate(ctx: Context<Undelegate>) -> Result<()> {
        require!(ctx.accounts.vault.delegated, VaultError::NotDelegated);

        let vault     = &ctx.accounts.vault;
        let owner_key = ctx.accounts.owner.key();
        let seeds: &[&[&[u8]]] = &[&[
            b"vault",
            owner_key.as_ref(),
            &vault.seed.to_le_bytes(),
            &[vault.bump],
        ]];

        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&UNDELEGATE_DISCRIMINATOR);

        let ix = Instruction {
            program_id: DELEGATION_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(ctx.accounts.owner.key(),             true),
                AccountMeta::new(ctx.accounts.vault.key(),             true),
                AccountMeta::new(ctx.accounts.delegation_buffer.key(), false),
                AccountMeta::new_readonly(DELEGATION_PROGRAM_ID,       false),
                AccountMeta::new_readonly(anchor_lang::solana_program::system_program::ID, false),
            ],
            data,
        };

        invoke_signed(
            &ix,
            &[
                ctx.accounts.owner.to_account_info(),
                ctx.accounts.vault.to_account_info(),
                ctx.accounts.delegation_program.to_account_info(),
                ctx.accounts.delegation_buffer.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            seeds,
        )?;

        // Reload to capture any final state the Delegation Program wrote
        ctx.accounts.vault.reload()?;

        let vault = &mut ctx.accounts.vault;
        vault.delegated        = false;
        vault.last_commit_slot = Clock::get()?.slot;

        emit!(VaultUndelegated {
            vault: vault.key(),
            slot:  vault.last_commit_slot,
        });

        Ok(())
    }

    // ── accrue (crank) ────────────────────────────────────────────────────────
    /// Permissionless crank instruction: anyone can call this to update
    /// `yield_accrued` on-chain. Useful for off-chain keepers or UI calls.
    /// When delegated, send to the rollup RPC so yield ticks at rollup speed.
    pub fn accrue(ctx: Context<Accrue>) -> Result<()> {
        accrue_yield(&mut ctx.accounts.vault)?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Yield calculation (pure helper — no account access)
// ─────────────────────────────────────────────────────────────────────────────

/// Accrues yield into `vault.yield_accrued` based on slots elapsed since
/// `vault.last_yield_slot` and the configured annual rate.
///
/// Formula (simple interest per slot, compounding on each call):
///   yield_delta = deposited × ANNUAL_YIELD_BPS × slots_elapsed
///                 ─────────────────────────────────────────────
///                     BPS_DENOMINATOR × SLOTS_PER_YEAR
fn accrue_yield(vault: &mut Account<Vault>) -> Result<()> {
    let current_slot  = Clock::get()?.slot;
    let slots_elapsed = current_slot.saturating_sub(vault.last_yield_slot);

    if slots_elapsed == 0 || vault.deposited == 0 {
        return Ok(());
    }

    // Numerator: deposited × rate_bps × slots_elapsed
    // Use u128 to avoid overflow before dividing
    let numerator = (vault.deposited as u128)
        .checked_mul(ANNUAL_YIELD_BPS as u128).ok_or(VaultError::Overflow)?
        .checked_mul(slots_elapsed   as u128).ok_or(VaultError::Overflow)?;

    let denominator = (BPS_DENOMINATOR as u128)
        .checked_mul(SLOTS_PER_YEAR as u128).ok_or(VaultError::Overflow)?;

    let yield_delta = (numerator / denominator) as u64;

    vault.yield_accrued = vault.yield_accrued
        .checked_add(yield_delta)
        .ok_or(VaultError::Overflow)?;
    vault.last_yield_slot = current_slot;

    emit!(YieldAccrued {
        vault: vault.key(),
        yield_delta,
        total_yield: vault.yield_accrued,
        slot:        current_slot,
    });

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Account validation structs
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init,
        payer  = owner,
        space  = Vault::LEN,
        seeds  = [b"vault", owner.key().as_ref(), &seed.to_le_bytes()],
        bump,
    )]
    pub vault: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds  = [b"vault", owner.key().as_ref(), &vault.seed.to_le_bytes()],
        bump   = vault.bump,
        has_one = owner @ VaultError::Unauthorized,
    )]
    pub vault: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds  = [b"vault", owner.key().as_ref(), &vault.seed.to_le_bytes()],
        bump   = vault.bump,
        has_one = owner @ VaultError::Unauthorized,
    )]
    pub vault: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Delegate<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds  = [b"vault", owner.key().as_ref(), &vault.seed.to_le_bytes()],
        bump   = vault.bump,
        has_one = owner @ VaultError::Unauthorized,
        // Ensure our program still owns the account (not already delegated)
        constraint = vault.to_account_info().owner == &crate::ID
            @ VaultError::AlreadyDelegated,
    )]
    pub vault: Account<'info, Vault>,

    /// Derived off-chain: seeds = ["delegation", vault_pubkey]
    /// under DELEGATION_PROGRAM_ID. Pass as writable — MagicBlock inits it.
    /// CHECK: Verified by the Delegation Program during the CPI.
    #[account(mut)]
    pub delegation_buffer: UncheckedAccount<'info>,

    /// CHECK: Constant address check enforced below.
    #[account(address = DELEGATION_PROGRAM_ID)]
    pub delegation_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Undelegate<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    /// While delegated the vault's program owner is DELEGATION_PROGRAM_ID,
    /// so we relax the owner check here — seeds + bump are sufficient.
    #[account(
        mut,
        seeds  = [b"vault", owner.key().as_ref(), &vault.seed.to_le_bytes()],
        bump   = vault.bump,
        has_one = owner @ VaultError::Unauthorized,
    )]
    pub vault: Account<'info, Vault>,

    /// CHECK: Verified by the Delegation Program during the CPI.
    #[account(mut)]
    pub delegation_buffer: UncheckedAccount<'info>,

    /// CHECK: Constant address check enforced below.
    #[account(address = DELEGATION_PROGRAM_ID)]
    pub delegation_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Accrue<'info> {
    #[account(mut)]
    pub vault: Account<'info, Vault>,
    // No signer required — permissionless crank
}

// ─────────────────────────────────────────────────────────────────────────────
// State
// ─────────────────────────────────────────────────────────────────────────────

#[account]
pub struct Vault {
    /// Wallet that owns this vault
    pub owner:            Pubkey,  // 32
    /// User-chosen seed (allows one wallet → many vaults)
    pub seed:             u64,     // 8
    /// Total lamports deposited (principal)
    pub deposited:        u64,     // 8
    /// Yield accrued since last settlement (in lamports)
    pub yield_accrued:    u64,     // 8
    /// True while the account is under ephemeral rollup control
    pub delegated:        bool,    // 1
    /// Slot when the last deposit occurred
    pub deposit_slot:     u64,     // 8
    /// Slot used as the start point for the next yield calculation
    pub last_yield_slot:  u64,     // 8
    /// Slot of the last delegation or undelegation event
    pub last_commit_slot: u64,     // 8
    /// PDA bump — stored to avoid recomputation in hot paths
    pub bump:             u8,      // 1
                                   // + 8 discriminator (Anchor prefix)
                                   // + 64 padding for future fields
}

impl Vault {
    pub const LEN: usize = 8   // discriminator
        + 32   // owner
        + 8    // seed
        + 8    // deposited
        + 8    // yield_accrued
        + 1    // delegated
        + 8    // deposit_slot
        + 8    // last_yield_slot
        + 8    // last_commit_slot
        + 1    // bump
        + 64;  // future-proof padding
}

// ─────────────────────────────────────────────────────────────────────────────
// Events
// ─────────────────────────────────────────────────────────────────────────────

#[event]
pub struct VaultInitialized {
    pub vault: Pubkey,
    pub owner: Pubkey,
    pub seed:  u64,
    pub slot:  u64,
}

#[event]
pub struct Deposited {
    pub vault:  Pubkey,
    pub amount: u64,
    pub total:  u64,
    pub slot:   u64,
}

#[event]
pub struct Withdrawn {
    pub vault:  Pubkey,
    pub amount: u64,
    pub slot:   u64,
}

#[event]
pub struct YieldAccrued {
    pub vault:       Pubkey,
    pub yield_delta: u64,
    pub total_yield: u64,
    pub slot:        u64,
}

#[event]
pub struct VaultDelegated {
    pub vault:            Pubkey,
    pub valid_until_slot: u64,
    pub slot:             u64,
}

#[event]
pub struct VaultUndelegated {
    pub vault: Pubkey,
    pub slot:  u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

#[error_code]
pub enum VaultError {
    #[msg("Vault is already delegated to the ephemeral rollup")]
    AlreadyDelegated,
    #[msg("Vault is not currently delegated")]
    NotDelegated,
    #[msg("Vault is still delegated — call undelegate first")]
    StillDelegated,
    #[msg("Signer is not the vault owner")]
    Unauthorized,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Insufficient funds in vault")]
    InsufficientFunds,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
}