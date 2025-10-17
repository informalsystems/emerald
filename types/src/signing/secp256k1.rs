use std::fmt;

use alloy_primitives::keccak256;
use async_trait::async_trait;
use bytes::Bytes;
use k256::ecdsa::signature::hazmat::{PrehashSigner, PrehashVerifier};
use k256::ecdsa::{Signature as K256Signature, SigningKey, VerifyingKey};
use malachitebft_core_types::{
    Context, SignedExtension, SignedProposal, SignedProposalPart, SignedVote, SigningProvider,
    SigningScheme,
};
use rand::{CryptoRng, RngCore};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use super::Hashable;
use crate::{Proposal, ProposalPart, Vote};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Secp256k1;

impl SigningScheme for Secp256k1 {
    type DecodingError = SignatureDecodingError;
    type Signature = Signature;
    type PublicKey = PublicKey;
    type PrivateKey = PrivateKey;

    fn decode_signature(bytes: &[u8]) -> Result<Self::Signature, Self::DecodingError> {
        Signature::from_bytes(bytes)
    }

    fn encode_signature(signature: &Self::Signature) -> Vec<u8> {
        signature.to_bytes().to_vec()
    }
}

#[derive(Debug, Error)]
pub enum SignatureDecodingError {
    #[error("invalid signature length: {0}")]
    InvalidLength(usize),
    #[error("invalid signature")]
    InvalidSignature,
}

#[derive(Clone)]
pub struct PrivateKey(SigningKey);

impl PrivateKey {
    pub fn generate<R>(mut rng: R) -> Self
    where
        R: RngCore + CryptoRng,
    {
        Self(SigningKey::random(&mut rng))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, k256::ecdsa::Error> {
        SigningKey::from_bytes(&bytes.into()).map(Self)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, k256::ecdsa::Error> {
        SigningKey::from_slice(bytes).map(Self)
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes().into()
    }

    pub fn inner(&self) -> &SigningKey {
        &self.0
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey::from(self.0.verifying_key())
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        let hash = keccak256(data);
        let sig: K256Signature = self
            .0
            .sign_prehash(hash.as_ref())
            .expect("signing with valid hash should not fail");
        Signature::from(sig)
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrivateKey").finish()
    }
}

impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(self.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let trimmed = s.trim_start_matches("0x");
        let bytes = hex::decode(trimmed).map_err(|e| de::Error::custom(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(de::Error::custom(format!(
                "expected 32-byte private key, got {} bytes",
                bytes.len()
            )));
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        PrivateKey::from_bytes(array).map_err(|_| de::Error::custom("invalid private key"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(VerifyingKey);

impl PublicKey {
    pub fn from_verifying_key(key: VerifyingKey) -> Self {
        Self(key)
    }

    pub fn from_sec1_bytes(bytes: &[u8]) -> Result<Self, k256::ecdsa::Error> {
        VerifyingKey::from_sec1_bytes(bytes).map(Self)
    }

    pub fn to_uncompressed_bytes(&self) -> [u8; 65] {
        let encoded = self.0.to_encoded_point(false);
        let bytes = encoded.as_bytes();
        let mut out = [0u8; 65];
        out.copy_from_slice(bytes);
        out
    }

    pub fn verify(&self, data: &[u8], signature: &Signature) -> bool {
        let hash = keccak256(data);
        match signature.as_k256() {
            Ok(sig) => self.0.verify_prehash(hash.as_ref(), &sig).is_ok(),
            Err(_) => false,
        }
    }

    pub fn inner(&self) -> &VerifyingKey {
        &self.0
    }
}

impl From<VerifyingKey> for PublicKey {
    fn from(value: VerifyingKey) -> Self {
        PublicKey::from_verifying_key(value)
    }
}

impl From<&VerifyingKey> for PublicKey {
    fn from(value: &VerifyingKey) -> Self {
        PublicKey::from_verifying_key(*value)
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(self.to_uncompressed_bytes()))
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let trimmed = s.trim_start_matches("0x");
        let bytes = hex::decode(trimmed).map_err(|e| de::Error::custom(e.to_string()))?;
        if bytes.len() != 65 {
            return Err(de::Error::custom(format!(
                "expected 65-byte uncompressed secp256k1 key, got {} bytes",
                bytes.len()
            )));
        }
        PublicKey::from_sec1_bytes(&bytes).map_err(|_| de::Error::custom("invalid public key"))
    }
}

impl Hashable for PublicKey {
    type Output = [u8; 32];

    fn hash(&self) -> Self::Output {
        let uncompressed = self.to_uncompressed_bytes();
        // drop the leading 0x04 prefix before hashing
        keccak256(&uncompressed[1..]).into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Signature([u8; 64]);

impl Signature {
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SignatureDecodingError> {
        if bytes.len() != 64 {
            return Err(SignatureDecodingError::InvalidLength(bytes.len()));
        }
        let mut array = [0u8; 64];
        array.copy_from_slice(bytes);
        let _ = K256Signature::try_from(array.as_slice())
            .map_err(|_| SignatureDecodingError::InvalidSignature)?;
        Ok(Signature(array))
    }

    fn as_k256(&self) -> Result<K256Signature, k256::ecdsa::Error> {
        K256Signature::try_from(self.0.as_slice())
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = SignatureDecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Signature::from_bytes(value)
    }
}

impl From<K256Signature> for Signature {
    fn from(value: K256Signature) -> Self {
        let bytes: [u8; 64] = value.to_bytes().into();
        Signature(bytes)
    }
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let trimmed = s.trim_start_matches("0x");
        let bytes = hex::decode(trimmed).map_err(|e| de::Error::custom(e.to_string()))?;
        Signature::from_bytes(&bytes[..]).map_err(de::Error::custom)
    }
}

#[derive(Debug)]
pub struct Secp256k1Provider {
    private_key: PrivateKey,
}

impl Secp256k1Provider {
    pub fn new(private_key: PrivateKey) -> Self {
        Self { private_key }
    }

    pub fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        self.private_key.sign(data)
    }

    pub fn verify(&self, data: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
        public_key.verify(data, signature)
    }
}

#[async_trait]
impl<C> SigningProvider<C> for Secp256k1Provider
where
    C: Context<
        Vote = Vote,
        Proposal = Proposal,
        ProposalPart = ProposalPart,
        Extension = Bytes,
        SigningScheme = Secp256k1,
    >,
{
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn sign_vote(&self, vote: C::Vote) -> SignedVote<C> {
        let signature = self.sign(&vote.to_sign_bytes());
        SignedVote::new(vote, signature)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn verify_signed_vote(
        &self,
        vote: &C::Vote,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> bool {
        public_key.verify(&vote.to_sign_bytes(), signature)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn sign_proposal(&self, proposal: C::Proposal) -> SignedProposal<C> {
        let signature = self.sign(&proposal.to_sign_bytes());
        SignedProposal::new(proposal, signature)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn verify_signed_proposal(
        &self,
        proposal: &C::Proposal,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> bool {
        public_key.verify(&proposal.to_sign_bytes(), signature)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn sign_proposal_part(&self, proposal_part: C::ProposalPart) -> SignedProposalPart<C> {
        let signature = self.sign(&proposal_part.to_sign_bytes());
        SignedProposalPart::new(proposal_part, signature)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn verify_signed_proposal_part(
        &self,
        proposal_part: &C::ProposalPart,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> bool {
        public_key.verify(&proposal_part.to_sign_bytes(), signature)
    }

    async fn sign_vote_extension(&self, _extension: C::Extension) -> SignedExtension<C> {
        unimplemented!()
    }

    async fn verify_signed_vote_extension(
        &self,
        _extension: &C::Extension,
        _signature: &Signature,
        _public_key: &PublicKey,
    ) -> bool {
        unimplemented!()
    }
}
