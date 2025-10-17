pub trait Hashable {
    type Output;
    fn hash(&self) -> Self::Output;
}

pub mod ed25519;
pub mod secp256k1;

pub use ed25519::{
    Ed25519, Ed25519Provider, PrivateKey as Ed25519PrivateKey, PublicKey as Ed25519PublicKey,
    Signature as Ed25519Signature,
};
pub use secp256k1::{
    PrivateKey as Secp256k1PrivateKey, PublicKey as Secp256k1PublicKey, Secp256k1,
    Secp256k1Provider, Signature as Secp256k1Signature,
    SignatureDecodingError as Secp256k1SignatureDecodingError,
};
