pub trait Hashable {
    type Output;
    fn hash(&self) -> Self::Output;
}

pub mod bls12381;
pub mod ed25519;
pub mod secp256k1;
