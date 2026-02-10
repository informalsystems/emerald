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

// IETF BLS ciphersuite for min-sig mode (signature in G1, public key in G2), with PoP.
// Ethereum consensus uses the companion min-pk ciphersuite below (signature in G2).
const DST_BLS_SIG_IN_G1_WITH_POP: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_";

// IETF BLS ciphersuite for min-pk mode (signature in G2, public key in G1), with PoP.
// This is the ciphersuite used by Ethereum consensus BLS signatures.
const DST_BLS_SIG_IN_G2_WITH_POP: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

pub trait BlsVariant: Clone + core::fmt::Debug + Eq + Ord + Send + Sync + 'static {
    type SecretKey: Clone + Send + Sync;
    type PublicKey;
    type Signature;

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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Signature<V: BlsVariant> {
    bytes: Vec<u8>,
    _marker: PhantomData<V>,
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PublicKey<V: BlsVariant> {
    bytes: Vec<u8>,
    _marker: PhantomData<V>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Bls12381<V: BlsVariant>(PhantomData<V>);

impl<V> SigningScheme for Bls12381<V>
where
    V: BlsVariant,
{
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

macro_rules! impl_bls_variant {
    ($variant:ident, $module:ident, $pk_len:expr, $sig_len:expr, $dst:expr) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
        pub struct $variant;

        impl BlsVariant for $variant {
            type SecretKey = $module::SecretKey;
            type PublicKey = $module::PublicKey;
            type Signature = $module::Signature;

            const PK_LEN: usize = $pk_len;
            const SIG_LEN: usize = $sig_len;
            const DST: &'static [u8] = $dst;

            fn key_gen(ikm: &[u8]) -> Result<Self::SecretKey, BLST_ERROR> {
                $module::SecretKey::key_gen(ikm, &[])
            }

            fn secret_key_from_bytes(bytes: &[u8]) -> Result<Self::SecretKey, BLST_ERROR> {
                $module::SecretKey::from_bytes(bytes)
            }

            fn secret_key_to_bytes(secret_key: &Self::SecretKey) -> Vec<u8> {
                secret_key.to_bytes().to_vec()
            }

            fn public_key_from_bytes(bytes: &[u8]) -> Result<Self::PublicKey, BLST_ERROR> {
                $module::PublicKey::from_bytes(bytes)
            }

            fn public_key_to_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
                public_key.to_bytes().to_vec()
            }

            fn public_key_from_secret_key(secret_key: &Self::SecretKey) -> Self::PublicKey {
                secret_key.sk_to_pk()
            }

            fn signature_from_bytes(bytes: &[u8]) -> Result<Self::Signature, BLST_ERROR> {
                $module::Signature::from_bytes(bytes)
            }

            fn signature_to_bytes(signature: &Self::Signature) -> Vec<u8> {
                signature.to_bytes().to_vec()
            }

            fn sign(secret_key: &Self::SecretKey, msg: &[u8]) -> Self::Signature {
                secret_key.sign(msg, Self::DST, &[])
            }

            fn verify(
                signature: &Self::Signature,
                msg: &[u8],
                public_key: &Self::PublicKey,
            ) -> BLST_ERROR {
                signature.verify(true, msg, Self::DST, &[], public_key, true)
            }
        }
    };
}

impl_bls_variant!(MinSig, min_sig, 96, 48, DST_BLS_SIG_IN_G1_WITH_POP);
impl_bls_variant!(MinPk, min_pk, 48, 96, DST_BLS_SIG_IN_G2_WITH_POP);

pub type Bls12381MinSig = Bls12381<MinSig>;
pub type Bls12381MinPk = Bls12381<MinPk>;
pub type BlsProviderMinSig = BlsProvider<MinSig>;
pub type BlsProviderMinPk = BlsProvider<MinPk>;

#[cfg(test)]
mod tests {
    use alloy_primitives::hex;
    use rand::rngs::OsRng;
    use rand::RngCore;

    use super::*;

    /*
    Source: ethereum/bls12-381-tests v0.1.2 release asset `bls_tests_json.tar.gz`
    URL: https://github.com/ethereum/bls12-381-tests/releases/tag/v0.1.2
    Fetch with:
    curl -sL "https://github.com/ethereum/bls12-381-tests/releases/download/v0.1.2/bls_tests_json.tar.gz" \
      | tar -xzO "./verify/verify_valid_case_195246ee3bd3b6ec.json" \
      | jq
    curl -sL "https://github.com/ethereum/bls12-381-tests/releases/download/v0.1.2/bls_tests_json.tar.gz" \
      | tar -xzO "./verify/verify_wrong_pubkey_case_195246ee3bd3b6ec.json" \
      | jq
    */
    const MESSAGE: [u8; 32] = hex!(
        "abababababababababababababababab"
        "abababababababababababababababab"
    );
    const PUBKEY: [u8; 48] = hex!(
        "b53d21a4cfd562c469cc81514d4ce5a6"
        "b577d8403d32a394dc265dd190b47fa9"
        "f829fdd7963afdf972e5e77854051f6f"
    );
    const SIGNATURE_VALID: [u8; 96] = hex!(
        "ae82747ddeefe4fd64cf9cedb9b04ae3"
        "e8a43420cd255e3c7cd06a8d88b7c7f8"
        "638543719981c5d16fa3527c468c25f0"
        "026704a6951bde891360c7e8d12ddee0"
        "559004ccdbe6046b55bae1b257ee97f7"
        "cdb955773d7cf29adf3ccbb9975e4eb9"
    );
    const SIGNATURE_WRONG_PUBKEY: [u8; 96] = hex!(
        "9674e2228034527f4c083206032b0203"
        "10face156d4a4685e2fcaec2f6f3665a"
        "a635d90347b6ce124eb879266b1e801d"
        "185de36a0a289b85e9039662634f2eea"
        "1e02e670bc7ab849d006a70b2f93b845"
        "97558a05b879c8d445f387a5d5b653df"
    );

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

    #[test]
    fn min_pk_ethereum_vector_verify_valid_case() {
        let public_key = PublicKey::<MinPk>::from_bytes(&PUBKEY).unwrap();
        let signature = Signature::<MinPk>::from_bytes(&SIGNATURE_VALID).unwrap();

        assert!(signature.verify(&MESSAGE, &public_key));
    }

    #[test]
    fn min_pk_ethereum_vector_verify_wrong_pubkey_case() {
        let public_key = PublicKey::<MinPk>::from_bytes(&PUBKEY).unwrap();
        let signature = Signature::<MinPk>::from_bytes(&SIGNATURE_WRONG_PUBKEY).unwrap();

        assert!(!signature.verify(&MESSAGE, &public_key));
    }
}
