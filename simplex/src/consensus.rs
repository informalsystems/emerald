//! Consensus types for simplex-based consensus.
//!
//! Uses secp256r1 signatures for consensus messages,
//! enabling gas-efficient on-chain verification via EIP-7212/RIP-7212.
//! The same secp256r1 keys are used for both P2P authentication and consensus signing.

use core::num::NonZero;

use commonware_consensus::simplex::scheme::secp256r1 as secp256r1_scheme;
use commonware_consensus::simplex::types::{
    Activity as CActivity, Finalization as CFinalization, Notarization as CNotarization,
};
use commonware_consensus::types::Epoch;
use commonware_cryptography::secp256r1::standard;
use commonware_cryptography::sha256::Digest;
use commonware_utils::NZU64;

/// The consensus signing scheme using secp256r1 signatures.
/// This enables gas-efficient on-chain verification via the RIP-7212 precompile.
pub type Scheme = secp256r1_scheme::Scheme<PublicKey>;

/// Notarization proof from consensus.
pub type Notarization = CNotarization<Scheme, Digest>;

/// Finalization proof from consensus.
pub type Finalization = CFinalization<Scheme, Digest>;

/// Consensus activity events.
pub type Activity = CActivity<Scheme, Digest>;

/// Public key type for node identity and consensus signing (secp256r1).
/// The same key is used for both P2P authentication and consensus.
pub type PublicKey = standard::PublicKey;

/// Private key type for signing.
pub type PrivateKey = standard::PrivateKey;

/// Namespace for consensus messages.
pub const NAMESPACE: &[u8] = b"emerald-simplex";

/// Epoch for validator set.
pub const EPOCH: Epoch = Epoch::new(0);

/// Number of views per epoch (as NonZero<u64>).
pub const EPOCH_LENGTH: NonZero<u64> = NZU64!(100_000);
