use core::marker::PhantomData;

use alloy_primitives::keccak256;
use async_trait::async_trait;
use blst::{min_pk, min_sig, BLST_ERROR};
use bytes::Bytes;
use malachitebft_core_types::{Context, SignedExtension, SignedMessage, SigningScheme};
use malachitebft_signing::{Error as SigningError, SigningProvider, VerificationResult};
use thiserror::Error;

use super::Hashable;
use crate::{Proposal, ProposalPart, Vote};

// BLS signatures ciphersuite for min-sig (sig in G1, pk in G2) with PoP.
// Ethereum CL uses the G2 variant of the same ciphersuite (G1 <-> G2 swapped).
const DST_BLS_SIG_IN_G1_WITH_POP: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_";

// BLS signatures ciphersuite for min-pk (sig in G2, pk in G1) with PoP.
// Ethereum CL uses the same ciphersuite string for its G2 signatures.
const DST_BLS_SIG_IN_G2_WITH_POP: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinSig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinPk;

pub trait BlsVariant: Copy + Send + Sync + 'static {
    type SecretKey: Clone + Send + Sync;
    type PublicKey: Clone + Send + Sync;
    type Signature: Clone + Send + Sync;

    const SK_LEN: usize = 32;
    const PK_LEN: usize;
    const SIG_LEN: usize;
    const DST: &'static [u8];

    fn key_gen(ikm: &[u8]) -> Result<Self::SecretKey, BLST_ERROR>;
    fn secret_key_from_bytes(bytes: &[u8]) -> Result<Self::SecretKey, BLST_ERROR>;
    fn secret_key_to_bytes(secret_key: &Self::SecretKey) -> Vec<u8>;

    fn public_key_from_bytes(bytes: &[u8]) -> Result<Self::PublicKey, BLST_ERROR>;
    fn public_key_to_bytes(public_key: &Self::PublicKey) -> Vec<u8>;
    fn public_key_from_secret_key(secret_key: &Self::SecretKey) -> Self::PublicKey;

    fn signature_from_bytes(bytes: &[u8]) -> Result<Self::Signature, BLST_ERROR>;
    fn signature_to_bytes(signature: &Self::Signature) -> Vec<u8>;

    fn sign(secret_key: &Self::SecretKey, msg: &[u8]) -> Self::Signature;
    fn verify(signature: &Self::Signature, msg: &[u8], public_key: &Self::PublicKey) -> BLST_ERROR;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
#[error("BLS decoding failed: {0:?}")]
pub struct BlsDecodingError(pub BLST_ERROR);

#[derive(Clone)]
pub struct Signature<V: BlsVariant> {
    bytes: Vec<u8>,
    _marker: PhantomData<V>,
}

impl<V: BlsVariant> core::fmt::Debug for Signature<V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Signature")
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl<V: BlsVariant> PartialEq for Signature<V> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl<V: BlsVariant> Eq for Signature<V> {}

impl<V: BlsVariant> PartialOrd for Signature<V> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<V: BlsVariant> Ord for Signature<V> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl<V: BlsVariant> Signature<V> {
    pub fn len() -> usize {
        V::SIG_LEN
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        if bytes.len() != V::SIG_LEN {
            return Err(BLST_ERROR::BLST_BAD_ENCODING);
        }
        let sig = V::signature_from_bytes(bytes)?;
        Ok(Self {
            bytes: V::signature_to_bytes(&sig).to_vec(),
            _marker: PhantomData,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    pub fn verify(&self, data: &[u8], public_key: &PublicKey<V>) -> bool {
        public_key.verify(data, self)
    }
}

#[derive(Clone)]
pub struct PublicKey<V: BlsVariant> {
    bytes: Vec<u8>,
    _marker: PhantomData<V>,
}

impl<V: BlsVariant> core::fmt::Debug for PublicKey<V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PublicKey")
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl<V: BlsVariant> PartialEq for PublicKey<V> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl<V: BlsVariant> Eq for PublicKey<V> {}

impl<V: BlsVariant> PartialOrd for PublicKey<V> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<V: BlsVariant> Ord for PublicKey<V> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl<V: BlsVariant> PublicKey<V> {
    pub fn len() -> usize {
        V::PK_LEN
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        if bytes.len() != V::PK_LEN {
            return Err(BLST_ERROR::BLST_BAD_ENCODING);
        }
        let pk = V::public_key_from_bytes(bytes)?;
        Ok(Self {
            bytes: V::public_key_to_bytes(&pk).to_vec(),
            _marker: PhantomData,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    pub fn verify(&self, data: &[u8], signature: &Signature<V>) -> bool {
        // TODO: avoid reparsing signature/public key bytes on every verify call; keep a parsed form
        // or cache decoded blst values for consensus hot paths.
        let Ok(sig) = V::signature_from_bytes(&signature.bytes) else {
            return false;
        };
        let Ok(pk) = V::public_key_from_bytes(&self.bytes) else {
            return false;
        };

        V::verify(&sig, data, &pk) == BLST_ERROR::BLST_SUCCESS
    }
}

#[derive(Clone)]
pub struct PrivateKey<V: BlsVariant> {
    inner: V::SecretKey,
}

impl<V: BlsVariant> PrivateKey<V> {
    pub const LENGTH: usize = V::SK_LEN;

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLST_ERROR> {
        let inner = V::secret_key_from_bytes(bytes)?;
        Ok(Self { inner })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        V::secret_key_to_bytes(&self.inner)
    }

    pub fn public_key(&self) -> PublicKey<V> {
        let pk = V::public_key_from_secret_key(&self.inner);
        PublicKey {
            bytes: V::public_key_to_bytes(&pk).to_vec(),
            _marker: PhantomData,
        }
    }

    pub fn sign(&self, data: &[u8]) -> Signature<V> {
        let sig = V::sign(&self.inner, data);
        Signature {
            bytes: V::signature_to_bytes(&sig).to_vec(),
            _marker: PhantomData,
        }
    }
}

impl<V: BlsVariant> Hashable for PublicKey<V> {
    type Output = [u8; 32];

    fn hash(&self) -> [u8; 32] {
        *keccak256(self.bytes.as_slice())
    }
}

#[derive(Clone, Copy, Default)]
pub struct Bls12381<V: BlsVariant>(PhantomData<V>);

impl<V: BlsVariant> core::fmt::Debug for Bls12381<V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Bls12381").finish()
    }
}

impl<V: BlsVariant> PartialEq for Bls12381<V> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<V: BlsVariant> Eq for Bls12381<V> {}

impl<V: BlsVariant> SigningScheme for Bls12381<V> {
    type DecodingError = BlsDecodingError;
    type Signature = Signature<V>;
    type PublicKey = PublicKey<V>;
    type PrivateKey = PrivateKey<V>;

    fn decode_signature(bytes: &[u8]) -> Result<Self::Signature, Self::DecodingError> {
        Signature::from_bytes(bytes).map_err(BlsDecodingError)
    }

    fn encode_signature(signature: &Self::Signature) -> Vec<u8> {
        signature.to_bytes()
    }
}

pub struct BlsProvider<V: BlsVariant> {
    private_key: PrivateKey<V>,
}

impl<V: BlsVariant> core::fmt::Debug for BlsProvider<V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BlsProvider").finish()
    }
}

impl<V: BlsVariant> BlsProvider<V> {
    pub fn new(private_key: PrivateKey<V>) -> Self {
        Self { private_key }
    }

    pub fn private_key(&self) -> &PrivateKey<V> {
        &self.private_key
    }

    pub fn sign(&self, data: &[u8]) -> Signature<V> {
        self.private_key.sign(data)
    }
}

#[async_trait]
impl<C, V> SigningProvider<C> for BlsProvider<V>
where
    C: Context<
        Vote = Vote,
        Proposal = Proposal,
        ProposalPart = ProposalPart,
        Extension = Bytes,
        SigningScheme = Bls12381<V>,
    >,
    V: BlsVariant,
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
        signature: &Signature<V>,
        public_key: &PublicKey<V>,
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
        signature: &Signature<V>,
        public_key: &PublicKey<V>,
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
        signature: &Signature<V>,
        public_key: &PublicKey<V>,
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
        _signature: &Signature<V>,
        _public_key: &PublicKey<V>,
    ) -> Result<VerificationResult, SigningError> {
        unimplemented!()
    }
}

impl BlsVariant for MinSig {
    type SecretKey = min_sig::SecretKey;
    type PublicKey = min_sig::PublicKey;
    type Signature = min_sig::Signature;

    const PK_LEN: usize = 96;
    const SIG_LEN: usize = 48;
    const DST: &'static [u8] = DST_BLS_SIG_IN_G1_WITH_POP;

    fn key_gen(ikm: &[u8]) -> Result<Self::SecretKey, BLST_ERROR> {
        min_sig::SecretKey::key_gen(ikm, &[])
    }

    fn secret_key_from_bytes(bytes: &[u8]) -> Result<Self::SecretKey, BLST_ERROR> {
        min_sig::SecretKey::from_bytes(bytes)
    }

    fn secret_key_to_bytes(secret_key: &Self::SecretKey) -> Vec<u8> {
        secret_key.to_bytes().to_vec()
    }

    fn public_key_from_bytes(bytes: &[u8]) -> Result<Self::PublicKey, BLST_ERROR> {
        min_sig::PublicKey::from_bytes(bytes)
    }

    fn public_key_to_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
        public_key.to_bytes().to_vec()
    }

    fn public_key_from_secret_key(secret_key: &Self::SecretKey) -> Self::PublicKey {
        secret_key.sk_to_pk()
    }

    fn signature_from_bytes(bytes: &[u8]) -> Result<Self::Signature, BLST_ERROR> {
        min_sig::Signature::from_bytes(bytes)
    }

    fn signature_to_bytes(signature: &Self::Signature) -> Vec<u8> {
        signature.to_bytes().to_vec()
    }

    fn sign(secret_key: &Self::SecretKey, msg: &[u8]) -> Self::Signature {
        secret_key.sign(msg, Self::DST, &[])
    }

    fn verify(signature: &Self::Signature, msg: &[u8], public_key: &Self::PublicKey) -> BLST_ERROR {
        signature.verify(true, msg, Self::DST, &[], public_key, true)
    }
}

impl BlsVariant for MinPk {
    type SecretKey = min_pk::SecretKey;
    type PublicKey = min_pk::PublicKey;
    type Signature = min_pk::Signature;

    const PK_LEN: usize = 48;
    const SIG_LEN: usize = 96;
    const DST: &'static [u8] = DST_BLS_SIG_IN_G2_WITH_POP;

    fn key_gen(ikm: &[u8]) -> Result<Self::SecretKey, BLST_ERROR> {
        min_pk::SecretKey::key_gen(ikm, &[])
    }

    fn secret_key_from_bytes(bytes: &[u8]) -> Result<Self::SecretKey, BLST_ERROR> {
        min_pk::SecretKey::from_bytes(bytes)
    }

    fn secret_key_to_bytes(secret_key: &Self::SecretKey) -> Vec<u8> {
        secret_key.to_bytes().to_vec()
    }

    fn public_key_from_bytes(bytes: &[u8]) -> Result<Self::PublicKey, BLST_ERROR> {
        min_pk::PublicKey::from_bytes(bytes)
    }

    fn public_key_to_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
        public_key.to_bytes().to_vec()
    }

    fn public_key_from_secret_key(secret_key: &Self::SecretKey) -> Self::PublicKey {
        secret_key.sk_to_pk()
    }

    fn signature_from_bytes(bytes: &[u8]) -> Result<Self::Signature, BLST_ERROR> {
        min_pk::Signature::from_bytes(bytes)
    }

    fn signature_to_bytes(signature: &Self::Signature) -> Vec<u8> {
        signature.to_bytes().to_vec()
    }

    fn sign(secret_key: &Self::SecretKey, msg: &[u8]) -> Self::Signature {
        secret_key.sign(msg, Self::DST, &[])
    }

    fn verify(signature: &Self::Signature, msg: &[u8], public_key: &Self::PublicKey) -> BLST_ERROR {
        signature.verify(true, msg, Self::DST, &[], public_key, true)
    }
}

pub type Bls12381MinSig = Bls12381<MinSig>;
pub type Bls12381MinPk = Bls12381<MinPk>;
pub type BlsProviderMinSig = BlsProvider<MinSig>;
pub type BlsProviderMinPk = BlsProvider<MinPk>;

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use rand::RngCore;

    use super::*;

    fn sign_and_verify_roundtrip<V: BlsVariant>() {
        let mut ikm = vec![0u8; V::SK_LEN];
        OsRng.fill_bytes(&mut ikm);
        let blst_key = V::key_gen(&ikm).expect("key_gen should succeed with 32 bytes");
        let private_key = PrivateKey::<V>::from_bytes(&V::secret_key_to_bytes(&blst_key)).unwrap();
        let public_key = private_key.public_key();
        let message = b"hello bls";

        let signature = private_key.sign(message);

        assert!(public_key.verify(message, &signature));
        assert!(signature.verify(message, &public_key));
    }

    #[test]
    fn min_sig_sign_and_verify_roundtrip() {
        sign_and_verify_roundtrip::<MinSig>();
    }

    #[test]
    fn min_pk_sign_and_verify_roundtrip() {
        sign_and_verify_roundtrip::<MinPk>();
    }
}
