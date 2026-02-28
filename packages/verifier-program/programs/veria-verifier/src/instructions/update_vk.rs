//! `update_vk` instruction.
//!
//! Rotates the registered Groth16 verification key hash on the
//! `VerifierConfig` PDA.  Restricted to the admin recorded at
//! `initialize` time via Anchor's `has_one = admin` constraint, which
//! enforces that `config.admin == admin.key()` at constraint-evaluation
//! time — independent of any check done inside the handler body.
//!
//! Rotating the vk also bumps `vk_epoch` so off-chain readers can detect
//! proofs that were generated against an older key and drop them before
//! paying the on-chain compute.

use anchor_lang::prelude::*;

use crate::errors::VerifierError;
use crate::state::{VerifierConfig, VkRotated};

/// Accounts required to rotate the verification key.
#[derive(Accounts)]
pub struct UpdateVk<'info> {
    /// Must equal `config.admin`.  Enforced by `has_one = admin`.
    pub admin: Signer<'info>,

    /// Global config PDA.
    ///
    /// `mut` so we can rewrite `vk_hash` and `vk_epoch`.
    #[account(
        mut,
        seeds = [VerifierConfig::SEED],
        bump = config.bump,
        has_one = admin @ VerifierError::UnauthorizedAdmin,
    )]
    pub config: Box<Account<'info, VerifierConfig>>,
}

/// Handler.
///
/// `new_vk_hash` is the SHA-256 of the rotated vk.  Callers that hold the
/// raw vk should hash it locally via `crate::utils::compute_vk_hash` so the
/// off-chain and on-chain digests align byte-for-byte.
pub fn handler_update_vk(ctx: Context<UpdateVk>, new_vk_hash: [u8; 32]) -> Result<()> {
    // Defensive check: even though `has_one` already enforces this, we
    // re-assert here so the handler body is auditable in isolation.
    require_keys_eq!(
        ctx.accounts.config.admin,
        ctx.accounts.admin.key(),
        VerifierError::UnauthorizedAdmin
    );

    // Reject no-op rotations so the on-chain event log stays meaningful.
    require!(
        ctx.accounts.config.vk_hash != new_vk_hash,
        VerifierError::VkMismatch
    );

    let config = &mut ctx.accounts.config;
    let old_vk_hash = config.vk_hash;

    config.vk_hash = new_vk_hash;
    config.vk_epoch = config
        .vk_epoch
        .checked_add(1)
        .ok_or(VerifierError::VkMismatch)?;

    emit!(VkRotated {
        admin: config.admin,
        old_vk_hash,
        new_vk_hash,
        new_epoch: config.vk_epoch,
    });

    msg!(
        "veria: vk rotated epoch={} old_hash[0..4]={:02x?}",
        config.vk_epoch,
        &old_vk_hash[..4]
    );

    Ok(())
}
