//! `initialize` instruction.
//!
//! Creates the singleton `VerifierConfig` PDA at `[b"config"]` and seeds it
//! with the admin authority, the active SP1 Groth16 verification key hash,
//! and the cluster prefix that future callers must echo in their public
//! inputs blob.  The PDA bump is captured at creation time so subsequent
//! instructions can sign without recomputing via
//! `Pubkey::find_program_address`.
//!
//! This instruction is idempotent in the strict sense: a successful run
//! creates the PDA; a second run fails inside the `init` macro because the
//! account already exists.  We deliberately do not use `init_if_needed`
//! here — re-initialising the global config would be a privilege escalation
//! attack vector (a fresh admin pubkey could be installed by anyone who
//! could pay rent).

use anchor_lang::prelude::*;

use crate::errors::VerifierError;
use crate::state::{VerifierConfig, VerifierInitialized};

/// Accounts required to bootstrap the verifier.
#[derive(Accounts)]
pub struct Initialize<'info> {
    /// Future admin of the program.  Pays for the rent of the config PDA
    /// and is recorded as the only key allowed to call `update_vk`.
    #[account(mut)]
    pub admin: Signer<'info>,

    /// Global config PDA — created here.
    ///
    /// Wrapped in `Box` so the on-stack `Accounts` struct stays well under
    /// Solana's 4 KiB stack frame limit even as more accounts are added in
    /// future instruction variants.
    #[account(
        init,
        payer = admin,
        space = 8 + VerifierConfig::LEN,
        seeds = [VerifierConfig::SEED],
        bump,
    )]
    pub config: Box<Account<'info, VerifierConfig>>,

    pub system_program: Program<'info, System>,
}

/// Handler.
///
/// `vk_hash` is the SHA-256 of the active SP1 Groth16 verification key.  We
/// take the hash (not the multi-KB raw key) because the raw key is bundled
/// inside `sp1-solana` and we only need to confirm the on-chain registry
/// agrees with the off-chain prover.
///
/// `cluster_label` is a short ASCII byte string identifying the target
/// cluster (`b"solana-mainnet-beta"`, `b"solana-devnet"`, ...).  The first
/// 8 bytes of `sha256(cluster_label)` are stored as the cluster prefix.
pub fn handler_initialize(
    ctx: Context<Initialize>,
    vk_hash: [u8; 32],
    cluster_label: Vec<u8>,
) -> Result<()> {
    require!(!cluster_label.is_empty(), VerifierError::EmptyPayload);
    require!(cluster_label.len() <= 64, VerifierError::InvalidPublicInputs);

    let cluster_prefix = crate::utils::cluster_prefix_for(&cluster_label);

    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.vk_hash = vk_hash;
    config.cluster_prefix = cluster_prefix;
    config.total_verified = 0;
    config.vk_epoch = 1;
    config.bump = ctx.bumps.config;
    config._padding = [0u8; 3];

    emit!(VerifierInitialized {
        admin: config.admin,
        vk_hash,
        cluster_prefix,
    });

    msg!(
        "veria: initialised cluster_prefix={:02x?} vk_epoch={}",
        cluster_prefix,
        config.vk_epoch
    );

    Ok(())
}
