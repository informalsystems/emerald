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
use commonware_cryptography::bls12381::primitives::variant::{MinSig, Variant};
use commonware_cryptography::ed25519;
use commonware_cryptography::sha256::Digest;
use commonware_utils::NZU64;

/// The consensus signing scheme using BLS12-381 threshold signatures.
pub type Scheme = bls12381_threshold::Scheme<PublicKey, MinSig>;

/// Seed for generating BLS keys.
pub type Seed = bls12381_threshold::Seed<MinSig>;

/// Notarization proof from consensus.
pub type Notarization = CNotarization<Scheme, Digest>;

/// Finalization proof from consensus.
pub type Finalization = CFinalization<Scheme, Digest>;

/// Consensus activity events.
pub type Activity = CActivity<Scheme, Digest>;

/// Public key type for node identity (Ed25519).
pub type PublicKey = ed25519::PublicKey;

/// BLS identity for threshold signing.
pub type Identity = <MinSig as Variant>::Public;

/// BLS signature type.
pub type Signature = <MinSig as Variant>::Signature;

/// Namespace for consensus messages.
pub const NAMESPACE: &[u8] = b"emerald-simplex";

/// Epoch for validator set.
pub const EPOCH: Epoch = Epoch::new(0);

/// Number of views per epoch (as NonZero<u64>).
pub const EPOCH_LENGTH: NonZero<u64> = NZU64!(100_000);
