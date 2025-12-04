use malachitebft_core_types::VotingPower;
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::secp256k1::PrivateKey;
use crate::Validator;

pub fn make_validators<const N: usize>(
    voting_powers: [VotingPower; N],
) -> [(Validator, PrivateKey); N] {
    let mut rng = StdRng::seed_from_u64(0x42);

    let mut validators = Vec::with_capacity(N);

    for vp in voting_powers {
        let sk = PrivateKey::generate(&mut rng);
        let val = Validator::new(sk.public_key(), vp);
        validators.push((val, sk));
    }

    validators.try_into().expect("N validators")
}

/// Generate validators with deterministic individual seeds for each validator.
/// This is used for MBT testing where each validator needs a specific seed.
/// Each validator uses seed_from_u64(index).
pub fn make_validators_with_individual_seeds<const N: usize>(
    voting_powers: [VotingPower; N],
) -> [(Validator, PrivateKey); N] {
    let mut validators = Vec::with_capacity(N);

    for (idx, vp) in voting_powers.iter().enumerate() {
        let mut rng = StdRng::seed_from_u64(idx as u64);
        let sk = PrivateKey::generate(&mut rng);
        let val = Validator::new(sk.public_key(), *vp);
        validators.push((val, sk));
    }

    validators.try_into().expect("N validators")
}
