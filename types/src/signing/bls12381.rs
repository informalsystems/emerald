use alloy_primitives::keccak256;
use async_trait::async_trait;
use blst::min_sig;
use blst::BLST_ERROR;
use bytes::Bytes;
use malachitebft_core_types::{Context, SignedExtension, SignedMessage, SigningScheme};
use malachitebft_signing::{Error as SigningError, SigningProvider, VerificationResult};

use super::Hashable;
use crate::{Proposal, ProposalPart, Vote};

const DST_BLS_SIG_IN_G1_WITH_POP: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bls12381;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlsDecodingError(pub BLST_ERROR);

impl core::fmt::Display for BlsDecodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Signature {
    bytes: [u8; Self::LENGTH],
}

impl Signature {
    pub const LENGTH: usize = 48;

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        let sig = min_sig::Signature::from_bytes(bytes)?;
        Ok(Self { bytes: sig.to_bytes() })
    }

    pub fn to_bytes(&self) -> [u8; Self::LENGTH] {
        self.bytes
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }

    pub fn verify(&self, data: &[u8], public_key: &PublicKey) -> bool {
        let sig = match min_sig::Signature::from_bytes(&self.bytes) {
            Ok(sig) => sig,
            Err(_) => return false,
        };
        let pk = match min_sig::PublicKey::from_bytes(&public_key.bytes) {
            Ok(pk) => pk,
            Err(_) => return false,
        };

        sig.verify(true, data, DST_BLS_SIG_IN_G1_WITH_POP, &[], &pk, false)
            == BLST_ERROR::BLST_SUCCESS
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PublicKey {
    bytes: [u8; Self::LENGTH],
}

impl PublicKey {
    pub const LENGTH: usize = 96;

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        let pk = min_sig::PublicKey::from_bytes(bytes)?;
        Ok(Self { bytes: pk.to_bytes() })
    }

    pub fn to_bytes(&self) -> [u8; Self::LENGTH] {
        self.bytes
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }
}

#[derive(Clone, Debug)]
pub struct PrivateKey {
    inner: min_sig::SecretKey,
}

impl PrivateKey {
    pub const LENGTH: usize = 32;

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        let inner = min_sig::SecretKey::from_bytes(bytes)?;
        Ok(Self { inner })
    }

    pub fn to_bytes(&self) -> [u8; Self::LENGTH] {
        self.inner.to_bytes()
    }

    pub fn public_key(&self) -> PublicKey {
        let pk = self.inner.sk_to_pk();
        PublicKey { bytes: pk.to_bytes() }
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        let sig = self.inner.sign(data, DST_BLS_SIG_IN_G1_WITH_POP, &[]);
        Signature { bytes: sig.to_bytes() }
    }
}

impl Hashable for PublicKey {
    type Output = [u8; 32];

    fn hash(&self) -> [u8; 32] {
        *keccak256(&self.bytes)
    }
}

impl SigningScheme for Bls12381 {
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

#[derive(Debug)]
pub struct BlsProvider {
    private_key: PrivateKey,
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
        SigningScheme = Bls12381,
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

