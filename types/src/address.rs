use core::fmt;

use alloy_primitives::{keccak256, Address as AlloyAddress};
use k256::ecdsa::VerifyingKey;
use malachitebft_proto::{Error as ProtoError, Protobuf};
use serde::{Deserialize, Serialize};

use crate::proto;
use crate::signing::secp256k1::PublicKey;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(
    AlloyAddress,
    // #[serde(
    //     serialize_with = "hex::serde::serialize_upper",
    //     deserialize_with = "hex::serde::deserialize"
    // )]
    // [u8; Self::LENGTH],
);

impl Address {
    const LENGTH: usize = 20;

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub const fn new(value: [u8; Self::LENGTH]) -> Self {
        Self(AlloyAddress::new(value))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn from_public_key(public_key: &PublicKey) -> Self {
        // Ethereum address derivation requires uncompressed public key
        // 1. Get public key bytes (compressed format from to_vec())
        // 2. Parse as k256 VerifyingKey to get uncompressed format
        // 3. Hash the uncompressed key (64 bytes without 0x04 prefix)
        // 4. Take the last 20 bytes

        let compressed_bytes = public_key.to_vec();

        // Parse the compressed key and get uncompressed encoding
        let verifying_key = VerifyingKey::from_sec1_bytes(&compressed_bytes)
            .expect("PublicKey to_vec() should always return valid SEC1 bytes");

        // Get uncompressed point (65 bytes: 0x04 || x || y)
        let uncompressed_point = verifying_key.to_encoded_point(false);
        let uncompressed_bytes = uncompressed_point.as_bytes();

        // Hash the x,y coordinates (skip the 0x04 prefix)
        let hash = keccak256(&uncompressed_bytes[1..]);

        // Take the last 20 bytes for Ethereum address
        let mut address = [0; Self::LENGTH];
        address.copy_from_slice(&hash[12..]);
        Self(AlloyAddress::new(address))
    }

    pub fn into_inner(self) -> [u8; Self::LENGTH] {
        self.0.into()
    }

    /// Creates a new [`FixedBytes`] where all bytes are set to `byte`.
    #[inline]
    pub const fn repeat_byte(byte: u8) -> Self {
        Self(AlloyAddress::repeat_byte(byte))
    }

    pub fn to_alloy_address(&self) -> alloy_primitives::Address {
        self.0
    }
}

impl fmt::Display for Address {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0.iter() {
            write!(f, "{byte:02X}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Address {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({self})")
    }
}

impl malachitebft_core_types::Address for Address {}

impl Protobuf for Address {
    type Proto = proto::Address;

    fn from_proto(proto: Self::Proto) -> Result<Self, ProtoError> {
        if proto.value.len() != Self::LENGTH {
            return Err(ProtoError::Other(format!(
                "Invalid address length: expected {}, got {}",
                Self::LENGTH,
                proto.value.len()
            )));
        }

        let mut address = [0; Self::LENGTH];
        address.copy_from_slice(&proto.value);
        Ok(Self(AlloyAddress::new(address)))
    }

    fn to_proto(&self) -> Result<Self::Proto, ProtoError> {
        Ok(proto::Address {
            value: self.0.to_vec().into(),
        })
    }
}

impl From<AlloyAddress> for Address {
    fn from(addr: AlloyAddress) -> Self {
        Self::new(addr.into())
    }
}
