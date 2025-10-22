use std::sync::Arc;

use alloy_primitives::{address, Address};
use alloy_provider::ProviderBuilder;
use color_eyre::eyre;
use malachitebft_eth_types::secp256k1::PublicKey;
use malachitebft_eth_types::{Validator, ValidatorSet};
use moka::future::Cache;

const VALIDATOR_SET_DELAY: u64 = 10;
const CACHE_CAPACITY: u64 = 1_000;

const GENESIS_VALIDATOR_MANAGER_ACCOUNT: Address =
    address!("0x0000000000000000000000000000000000002000");

alloy_sol_types::sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ValidatorManager,
    "../solidity/out/ValidatorManager.sol/ValidatorManager.json"
);

/// Simple error wrapper so we can store errors in the cache as Strings.
// We store errors as Strings directly in the cache values; no dedicated struct required.

#[derive(Clone)]
pub struct ValidatorSetManager {
    /// An async cache that supports single-flight fetches via `get_with`.
    /// The cache stores either Ok(Arc<ValidatorSet>) or Err(String) wrapped in Arc.
    cache: Arc<Cache<u64, Arc<Result<Arc<ValidatorSet>, String>>>>,
}

impl ValidatorSetManager {
    /// Create a new manager with a provider URL and cache capacity.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Cache::new(CACHE_CAPACITY)),
        }
    }
}

impl ValidatorSetManager {
    /// Start processing a height asynchronously. This method returns quickly;
    /// the actual work of fetching the validator set is performed on a spawned
    /// background task so it doesn't block the caller. If a fetch for the same
    /// height is already in progress, this is a no-op.
    /// Start processing a height asynchronously. Returns immediately. Uses
    /// `moka::future::Cache::get_with` to ensure single-flight fetching: the
    /// first caller will execute the async loader and others will await it.
    pub async fn process_height(&self, eth_url: &str, height: u64) {
        // If already cached, nothing to do.
        if self.cache.get(&height).await.is_some() {
            return;
        }

        let cache = self.cache.clone();
        let this = self.clone();
        let eth_url = eth_url.to_string();

        // spawn background task that performs a get_with and stores the result
        tokio::spawn(async move {
            // `get_with` will run the closure only once for concurrent callers.
            let _ = cache
                .get_with(height, async move {
                    match this.read_validators_from_contract(&eth_url, height).await {
                        Ok(vs) => Arc::new(Ok(Arc::new(vs))),
                        Err(e) => {
                            tracing::error!(%height, error = ?e, "failed to fetch validator set");
                            Arc::new(Err(format!("{}", e)))
                        }
                    }
                })
                .await;
        });
    }

    /// Fetch a height synchronously from the point of view of the caller: if the
    /// validator set for `height` is already available it is returned immediately.
    /// Otherwise this method will await until a background fetch started by
    /// `process_height` completes and returns the value. If no background fetch is
    /// in progress, this method will start one and then wait for it.
    /// Fetch a validator set for `height`, waiting for an in-progress fetch if
    /// necessary. Uses the cache single-flight semantics so concurrent callers
    /// for the same height share the same loader.
    /// Fetch a validator set for `height`, returning an error if the loader failed.
    pub async fn fetch_height(&self, eth_url: &str, height: u64) -> eyre::Result<ValidatorSet> {
        // If already cached, return clone immediately.
        if let Some(arc_res) = self.cache.get(&height).await {
            match arc_res.as_ref() {
                Ok(vs_arc) => return Ok((**vs_arc).clone()),
                Err(msg) => return Err(eyre::eyre!(msg.clone())),
            }
        }

        // Use get_with to ensure only one loader runs and others await it.
        let cache = self.cache.clone();
        let this = self.clone();
        let arc_res = cache
            .get_with(height, async move {
                match this.read_validators_from_contract(eth_url, height).await {
                    Ok(vs) => Arc::new(Ok(Arc::new(vs))),
                    Err(e) => {
                        tracing::error!(%height, error = ?e, "failed to fetch validator set");
                        Arc::new(Err(format!("{}", e)))
                    }
                }
            })
            .await;

        match arc_res.as_ref() {
            Ok(vs_arc) => Ok((**vs_arc).clone()),
            Err(msg) => Err(eyre::eyre!(msg.clone())),
        }
    }

    pub async fn fetch_height_with_delay(
        &self,
        eth_url: &str,
        height: u64,
    ) -> eyre::Result<ValidatorSet> {
        self.fetch_height(eth_url, height.saturating_sub(VALIDATOR_SET_DELAY))
            .await
    }

    // No clone_for_spawn required because the Manager derives Clone and its
    // inner state is Arc-wrapped so cloning the manager shares the same state.
    pub async fn read_validators_from_contract(
        &self,
        eth_url: &str,
        block_number: u64,
    ) -> eyre::Result<ValidatorSet> {
        let provider = ProviderBuilder::new().on_builtin(eth_url).await?;

        let validator_manager_contract =
            ValidatorManager::new(GENESIS_VALIDATOR_MANAGER_ACCOUNT, provider);

        let genesis_validator_set_sol = validator_manager_contract
            .getValidators()
            .block(block_number.into())
            .call()
            .await?;

        let validators = genesis_validator_set_sol
            .validators
            .into_iter()
            .map(
                |ValidatorManager::ValidatorInfo {
                     validatorKey,
                     power,
                 }| {
                    let mut uncompressed = [0u8; 65];
                    uncompressed[0] = 0x04;
                    uncompressed[1..33].copy_from_slice(&validatorKey.x.to_be_bytes::<32>());
                    uncompressed[33..].copy_from_slice(&validatorKey.y.to_be_bytes::<32>());

                    let pub_key = PublicKey::from_sec1_bytes(&uncompressed)?;

                    Ok(Validator::new(pub_key, power))
                },
            )
            .collect::<eyre::Result<Vec<_>>>()?;

        Ok(ValidatorSet::new(validators))
    }
}
