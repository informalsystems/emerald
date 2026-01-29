//! Consensus types for simplex-based consensus.
//!
//! Uses BLS12-381 threshold signatures for consensus messages,
//! matching the commonware simplex scheme.

use core::num::NonZero;

use commonware_consensus::simplex::scheme::bls12381_threshold;
pub use commonware_consensus::simplex::scheme::bls12381_threshold::Seedable;
use commonware_consensus::simplex::types::{
    Activity as CActivity, Finalization as CFinalization, Notarization as CNotarization,
};
use commonware_consensus::types::Epoch;
use commonware_cryptography::bls12381::primitives::variant::MinSig;
use commonware_cryptography::ed25519;
use commonware_cryptography::sha256::Digest;
use commonware_utils::NZU64;

/// The consensus signing scheme using BLS12-381 threshold signatures.
pub type Scheme = bls12381_threshold::Scheme<PublicKey, MinSig>;

/// Notarization proof from consensus.
pub type Notarization = CNotarization<Scheme, Digest>;

/// Finalization proof from consensus.
pub type Finalization = CFinalization<Scheme, Digest>;

/// Consensus activity events.
pub type Activity = CActivity<Scheme, Digest>;

/// Public key type for node identity (Ed25519).
pub type PublicKey = ed25519::PublicKey;

/// Namespace for consensus messages.
pub const NAMESPACE: &[u8] = b"emerald-simplex";

/// Epoch for validator set.
pub const EPOCH: Epoch = Epoch::new(0);

/// Number of views per epoch (as NonZero<u64>).
pub const EPOCH_LENGTH: NonZero<u64> = NZU64!(100_000);
