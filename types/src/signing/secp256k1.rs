use alloy_primitives::keccak256;
use async_trait::async_trait;
use bytes::Bytes;
use malachitebft_core_types::{Context, SignedExtension, SignedMessage};
use malachitebft_signing::{Error as SigningError, SigningProvider, VerificationResult};
use malachitebft_signing_ecdsa::K256Config;
pub use malachitebft_signing_ecdsa::{
    PrivateKey as EcdsaPrivateKey, PublicKey as EcdsaPublicKey, Signature as EcdsaSignature, K256,
};

use super::Hashable;
use crate::{Proposal, ProposalPart, Vote};

pub type PrivateKey = EcdsaPrivateKey<K256Config>;
pub type PublicKey = EcdsaPublicKey<K256Config>;
pub type Signature = EcdsaSignature<K256Config>;

impl Hashable for PublicKey {
    type Output = [u8; 32];

    fn hash(&self) -> [u8; 32] {
        keccak256(self.to_vec()).into()
    }
}

#[derive(Debug)]
pub struct K256Provider {
    private_key: PrivateKey,
}

impl K256Provider {
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
        public_key.verify(data, signature).is_ok()
    }
}

#[async_trait]
impl<C> SigningProvider<C> for K256Provider
where
    C: Context<
        Vote = Vote,
        Proposal = Proposal,
        ProposalPart = ProposalPart,
        Extension = Bytes,
        SigningScheme = K256,
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
            public_key.verify(&vote.to_sign_bytes(), signature).is_ok(),
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
            public_key
                .verify(&proposal.to_sign_bytes(), signature)
                .is_ok(),
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
            public_key
                .verify(&proposal_part.to_sign_bytes(), signature)
                .is_ok(),
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
