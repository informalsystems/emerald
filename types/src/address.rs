use core::fmt;

use alloy_primitives::Address as AlloyAddress;
use malachitebft_proto::{Error as ProtoError, Protobuf};
use serde::{Deserialize, Serialize};

use crate::signing::secp256k1::PublicKey;
use crate::{proto, Hashable};

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
        // Hash (keccak256) of the x and y coordinates of the public key
        let hash = public_key.hash();

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

#[cfg(test)]
mod tests {
    use alloy_primitives::{address, b256};

    use super::*;
    use crate::secp256k1::PrivateKey;

    #[test]
    fn test_ethereum_address_derivation_anvil_account() {
        // Anvil test account #0
        // Private key: 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
        // Expected address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266

        let private_key_bytes =
            b256!("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80");
        let expected_address = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");

        // Create PrivateKey from bytes
        let private_key = PrivateKey::from_slice(private_key_bytes.as_ref()).unwrap();
        let public_key = private_key.public_key();

        // Derive address
        let derived_address = Address::from_public_key(&public_key);

        assert_eq!(
            derived_address.to_alloy_address(),
            expected_address,
            "Derived address doesn't match expected Anvil address",
        );
    }
}
