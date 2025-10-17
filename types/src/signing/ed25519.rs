use async_trait::async_trait;
use bytes::Bytes;
use malachitebft_core_types::{
    Context, SignedExtension, SignedProposal, SignedProposalPart, SignedVote, SigningProvider,
};
pub use malachitebft_signing_ed25519::{Ed25519, PrivateKey, PublicKey, Signature};

use super::Hashable;
use crate::{Proposal, ProposalPart, Vote};

impl Hashable for PublicKey {
    type Output = [u8; 32];

    fn hash(&self) -> [u8; 32] {
        use sha3::{Digest, Keccak256};
        let mut hasher = Keccak256::new();
        hasher.update(self.as_bytes());
        hasher.finalize().into()
    }
}

#[derive(Debug)]
pub struct Ed25519Provider {
    private_key: PrivateKey,
}

impl Ed25519Provider {
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
impl<C> SigningProvider<C> for Ed25519Provider
where
    C: Context<
        Vote = Vote,
        Proposal = Proposal,
        ProposalPart = ProposalPart,
        Extension = Bytes,
        SigningScheme = Ed25519,
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
        public_key.verify(&vote.to_sign_bytes(), signature).is_ok()
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn sign_proposal(&self, proposal: C::Proposal) -> SignedProposal<C> {
        let signature = self.private_key.sign(&proposal.to_sign_bytes());
        SignedProposal::new(proposal, signature)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn verify_signed_proposal(
        &self,
        proposal: &C::Proposal,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> bool {
        public_key
            .verify(&proposal.to_sign_bytes(), signature)
            .is_ok()
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn sign_proposal_part(&self, proposal_part: C::ProposalPart) -> SignedProposalPart<C> {
        let signature = self.private_key.sign(&proposal_part.to_sign_bytes());
        SignedProposalPart::new(proposal_part, signature)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn verify_signed_proposal_part(
        &self,
        proposal_part: &C::ProposalPart,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> bool {
        public_key
            .verify(&proposal_part.to_sign_bytes(), signature)
            .is_ok()
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
