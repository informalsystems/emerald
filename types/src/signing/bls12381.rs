use core::marker::PhantomData;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinSig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinPk;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bls12381<V>(PhantomData<V>);

impl<V> Default for Bls12381<V> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

pub type Bls12381MinSig = Bls12381<MinSig>;
pub type Bls12381MinPk = Bls12381<MinPk>;
