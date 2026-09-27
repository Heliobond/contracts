#![no_std]

//! Shared multi-sig admin approval logic (issue #459), used by both
//! `investment_vault` and `project_registry`. Each caller maps the returned
//! error variant to its own `contracterror` enum, so this crate is agnostic
//! to which contract's error codes end up on-chain.

use soroban_sdk::{Address, Vec};

pub enum ConfigError {
    TooManySigners,
    InvalidThreshold,
    DuplicateSigner,
}

pub enum ApprovalError {
    InvalidThreshold,
    DuplicateApproval,
    NotSigner,
    InsufficientApprovals,
}

/// Validates a proposed signer set + threshold before storing it.
pub fn validate_multisig_config(
    signers: &Vec<Address>,
    threshold: u32,
    max_signers: u32,
) -> Result<(), ConfigError> {
    if signers.len() > max_signers {
        return Err(ConfigError::TooManySigners);
    }
    if threshold == 0 || threshold > signers.len() {
        return Err(ConfigError::InvalidThreshold);
    }
    for i in 0..signers.len() {
        let signer = signers.get(i).unwrap();
        for j in (i + 1)..signers.len() {
            if signer == signers.get(j).unwrap() {
                return Err(ConfigError::DuplicateSigner);
            }
        }
    }
    Ok(())
}

/// Enforces multi-sig approval for an admin action. When `threshold` is 0
/// (multi-sig disabled), falls back to requiring `owner`'s auth directly.
/// Calls `require_auth()` on each distinct approver that is a registered
/// signer.
pub fn require_admin_approval(
    owner: &Address,
    threshold: u32,
    signers: &Vec<Address>,
    approvals: Vec<Address>,
) -> Result<(), ApprovalError> {
    if threshold == 0 {
        owner.require_auth();
        return Ok(());
    }
    if threshold > signers.len() {
        return Err(ApprovalError::InvalidThreshold);
    }

    let mut approved = 0u32;
    for i in 0..approvals.len() {
        let approver = approvals.get(i).unwrap();
        for j in 0..i {
            if approver == approvals.get(j).unwrap() {
                return Err(ApprovalError::DuplicateApproval);
            }
        }

        let mut is_signer = false;
        for signer in signers.iter() {
            if approver == signer {
                is_signer = true;
                break;
            }
        }
        if !is_signer {
            return Err(ApprovalError::NotSigner);
        }
        approver.require_auth();
        approved += 1;
    }

    if approved < threshold {
        return Err(ApprovalError::InsufficientApprovals);
    }
    Ok(())
}

/// Returns `false` (multi-sig is enabled) when `threshold > 0`.
pub fn is_multisig_disabled(threshold: u32) -> bool {
    threshold == 0
}
