use alloy_primitives::keccak256;
use async_trait::async_trait;
use blst::{min_pk, BLST_ERROR};
use bytes::Bytes;
use malachitebft_core_types::{Context, SignedExtension, SignedMessage, SigningScheme};
use malachitebft_signing::{Error as SigningError, SigningProvider, VerificationResult};
use thiserror::Error;

use super::bls12381::{Bls12381, MinPk};
use super::Hashable;
use crate::{Proposal, ProposalPart, Vote};

// BLS signatures ciphersuite for min-pk (sig in G2, pk in G1) with PoP.
// Ethereum CL uses the same ciphersuite string for its G2 signatures.
const DST_BLS_SIG_IN_G2_WITH_POP: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
#[error("BLS decoding failed: {0:?}")]
pub struct BlsDecodingError(pub BLST_ERROR);

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Signature {
    bytes: [u8; Self::LENGTH],
}

impl Signature {
    pub const LENGTH: usize = 96;

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        let sig = min_pk::Signature::from_bytes(bytes)?;
        Ok(Self {
            bytes: sig.to_bytes(),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::LENGTH] {
        self.bytes
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }

    pub fn verify(&self, data: &[u8], public_key: &PublicKey) -> bool {
        public_key.verify(data, self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PublicKey {
    bytes: [u8; Self::LENGTH],
}

impl PublicKey {
    pub const LENGTH: usize = 48;

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        let pk = min_pk::PublicKey::from_bytes(bytes)?;
        Ok(Self {
            bytes: pk.to_bytes(),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::LENGTH] {
        self.bytes
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }

    pub fn verify(&self, data: &[u8], signature: &Signature) -> bool {
        let Ok(sig) = min_pk::Signature::from_bytes(&signature.bytes) else {
            return false;
        };
        let Ok(pk) = min_pk::PublicKey::from_bytes(&self.bytes) else {
            return false;
        };

        sig.verify(true, data, DST_BLS_SIG_IN_G2_WITH_POP, &[], &pk, true)
            == BLST_ERROR::BLST_SUCCESS
    }
}

#[derive(Clone)]
pub struct PrivateKey {
    inner: min_pk::SecretKey,
}

impl PrivateKey {
    pub const LENGTH: usize = 32;

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        let inner = min_pk::SecretKey::from_bytes(bytes)?;
        Ok(Self { inner })
    }

    pub fn to_bytes(&self) -> [u8; Self::LENGTH] {
        self.inner.to_bytes()
    }

    pub fn public_key(&self) -> PublicKey {
        let pk = self.inner.sk_to_pk();
        PublicKey {
            bytes: pk.to_bytes(),
        }
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        let sig = self.inner.sign(data, DST_BLS_SIG_IN_G2_WITH_POP, &[]);
        Signature {
            bytes: sig.to_bytes(),
        }
    }
}

impl Hashable for PublicKey {
    type Output = [u8; 32];

    fn hash(&self) -> [u8; 32] {
        *keccak256(self.bytes)
    }
}

impl SigningScheme for Bls12381<MinPk> {
    type DecodingError = BlsDecodingError;
    type Signature = Signature;
    type PublicKey = PublicKey;
    type PrivateKey = PrivateKey;

    fn decode_signature(bytes: &[u8]) -> Result<Self::Signature, Self::DecodingError> {
        Signature::from_bytes(bytes).map_err(BlsDecodingError)
    }

    fn encode_signature(signature: &Self::Signature) -> Vec<u8> {
        signature.to_vec()
    }
}

pub struct BlsProvider {
    private_key: PrivateKey,
}

impl core::fmt::Debug for BlsProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BlsProvider").finish()
    }
}

impl BlsProvider {
    pub fn new(private_key: PrivateKey) -> Self {
        Self { private_key }
    }

    pub fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        self.private_key.sign(data)
    }
}

#[async_trait]
impl<C> SigningProvider<C> for BlsProvider
where
    C: Context<
        Vote = Vote,
        Proposal = Proposal,
        ProposalPart = ProposalPart,
        Extension = Bytes,
        SigningScheme = Bls12381<MinPk>,
    >,
{
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn sign_vote(&self, vote: C::Vote) -> Result<SignedMessage<C, C::Vote>, SigningError> {
        let signature = self.sign(&vote.to_sign_bytes());
        Ok(SignedMessage::new(vote, signature))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn verify_signed_vote(
        &self,
        vote: &C::Vote,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<VerificationResult, SigningError> {
        Ok(VerificationResult::from_bool(
            signature.verify(&vote.to_sign_bytes(), public_key),
        ))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn sign_proposal(
        &self,
        proposal: C::Proposal,
    ) -> Result<SignedMessage<C, C::Proposal>, SigningError> {
        let signature = self.sign(&proposal.to_sign_bytes());
        Ok(SignedMessage::new(proposal, signature))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn verify_signed_proposal(
        &self,
        proposal: &C::Proposal,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<VerificationResult, SigningError> {
        Ok(VerificationResult::from_bool(
            signature.verify(&proposal.to_sign_bytes(), public_key),
        ))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn sign_proposal_part(
        &self,
        proposal_part: C::ProposalPart,
    ) -> Result<SignedMessage<C, C::ProposalPart>, SigningError> {
        let signature = self.sign(&proposal_part.to_sign_bytes());
        Ok(SignedMessage::new(proposal_part, signature))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn verify_signed_proposal_part(
        &self,
        proposal_part: &C::ProposalPart,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<VerificationResult, SigningError> {
        Ok(VerificationResult::from_bool(
            signature.verify(&proposal_part.to_sign_bytes(), public_key),
        ))
    }

    async fn sign_vote_extension(
        &self,
        _extension: C::Extension,
    ) -> Result<SignedExtension<C>, SigningError> {
        unimplemented!()
    }

    async fn verify_signed_vote_extension(
        &self,
        _extension: &C::Extension,
        _signature: &Signature,
        _public_key: &PublicKey,
    ) -> Result<VerificationResult, SigningError> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use rand::RngCore;

    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() {
        let mut ikm = [0u8; PrivateKey::LENGTH];
        OsRng.fill_bytes(&mut ikm);
        let blst_key =
            min_pk::SecretKey::key_gen(&ikm, &[]).expect("key_gen should succeed with 32 bytes");
        let private_key = PrivateKey::from_bytes(&blst_key.to_bytes()).unwrap();
        let public_key = private_key.public_key();
        let message = b"hello bls";

        let signature = private_key.sign(message);

        assert!(public_key.verify(message, &signature));
        assert!(signature.verify(message, &public_key));
    }
}
