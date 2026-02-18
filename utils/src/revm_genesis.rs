use std::collections::{BTreeMap, HashSet};

use alloy_genesis::GenesisAccount;
use alloy_primitives::{address, Address, Bytes, U256};
use alloy_sol_types::{SolCall, SolConstructor};
use color_eyre::eyre::{eyre, Result};
use revm::context::{Context, TxEnv};
use revm::context_interface::ContextTr;
use revm::database::InMemoryDB;
use revm::handler::{ExecuteCommitEvm, MainBuilder, MainContext};
use revm::state::AccountInfo;

use crate::validator_manager::{Validator, ValidatorManager, ValidatorManagerProxy};

// Deployer address for genesis contracts via CREATE (not CREATE2).
// Address: 0x0000000000000000000000000000000000000001
// Nonce ordering: impl (0), proxy (1), initialize (2), owner (0..N)
const GENESIS_DEPLOYER: Address = address!("0x0000000000000000000000000000000000000001");
const GENESIS_DEPLOYER_BALANCE: U256 = U256::from_limbs([u64::MAX, u64::MAX, u64::MAX, 0]);
const GENESIS_TX_GAS_LIMIT: u64 = 15_000_000;

#[derive(Debug, Clone)]
pub struct ValidatorManagerGenesisAlloc {
    pub proxy_address: Address,
    pub implementation_address: Address,
    pub alloc: BTreeMap<Address, GenesisAccount>,
}

pub fn build_validator_manager_alloc_via_revm(
    validators: &[Validator],
    owner: Address,
) -> Result<ValidatorManagerGenesisAlloc> {
    validate_validators(validators)?;

    let mut db = InMemoryDB::default();
    db.insert_account_info(
        GENESIS_DEPLOYER,
        AccountInfo {
            balance: GENESIS_DEPLOYER_BALANCE,
            nonce: 0,
            ..Default::default()
        },
    );
    if owner != GENESIS_DEPLOYER {
        db.insert_account_info(
            owner,
            AccountInfo {
                balance: GENESIS_DEPLOYER_BALANCE,
                nonce: 0,
                ..Default::default()
            },
        );
    }

    let mut evm = Context::mainnet().with_db(db).build_mainnet();
    let mut deployer_nonce = 0u64;

    // Deploy implementation via CREATE (nonce 0)
    let implementation_address = {
        let implementation_init_code = ValidatorManager::BYTECODE.clone();
        let tx = TxEnv::builder()
            .caller(GENESIS_DEPLOYER)
            .nonce(deployer_nonce)
            .gas_limit(GENESIS_TX_GAS_LIMIT)
            .create()
            .data(implementation_init_code)
            .build()
            .map_err(|e| eyre!("failed to build implementation CREATE tx: {e:?}"))?;

        let result = evm
            .transact_commit(tx)
            .map_err(|e| eyre!("implementation CREATE tx failed: {e:?}"))?;

        if !result.is_success() {
            return Err(eyre!("implementation CREATE tx reverted: {result:?}"));
        }

        deployer_nonce += 1;
        result
            .created_address()
            .ok_or_else(|| eyre!("implementation CREATE tx missing created address"))?
    };

    // Deploy proxy via CREATE (nonce 1)
    let proxy_address = {
        let constructor_args = ValidatorManagerProxy::constructorCall {
            implementation: implementation_address,
            _data: Bytes::new(),
        }
        .abi_encode();

        let mut proxy_init_code = ValidatorManagerProxy::BYTECODE.to_vec();
        proxy_init_code.extend_from_slice(&constructor_args);
        let proxy_init_code: Bytes = proxy_init_code.into();

        let tx = TxEnv::builder()
            .caller(GENESIS_DEPLOYER)
            .nonce(deployer_nonce)
            .gas_limit(GENESIS_TX_GAS_LIMIT)
            .create()
            .data(proxy_init_code)
            .build()
            .map_err(|e| eyre!("failed to build proxy CREATE tx: {e:?}"))?;

        let result = evm
            .transact_commit(tx)
            .map_err(|e| eyre!("proxy CREATE tx failed: {e:?}"))?;

        if !result.is_success() {
            return Err(eyre!("proxy CREATE tx reverted: {result:?}"));
        }

        deployer_nonce += 1;
        result
            .created_address()
            .ok_or_else(|| eyre!("proxy CREATE tx missing created address"))?
    };

    let initialize_calldata = ValidatorManager::initializeCall {
        initialOwner: owner,
    }
    .abi_encode();

    let initialize_tx = TxEnv::builder()
        .caller(GENESIS_DEPLOYER)
        .nonce(deployer_nonce)
        .gas_limit(GENESIS_TX_GAS_LIMIT)
        .call(proxy_address)
        .data(initialize_calldata.into())
        .build()
        .map_err(|e| eyre!("failed to build initialize CALL tx: {e:?}"))?;

    let initialize_result = evm
        .transact_commit(initialize_tx)
        .map_err(|e| eyre!("initialize CALL tx failed: {e:?}"))?;

    if !initialize_result.is_success() {
        return Err(eyre!("initialize CALL tx reverted: {initialize_result:?}"));
    }

    for (owner_nonce, validator) in validators.iter().enumerate() {
        let mut pubkey = Vec::with_capacity(65);
        pubkey.push(0x04);
        pubkey.extend_from_slice(&validator.validator_key.0.to_be_bytes::<32>());
        pubkey.extend_from_slice(&validator.validator_key.1.to_be_bytes::<32>());

        let calldata = ValidatorManager::registerCall {
            validatorPublicKey: pubkey.into(),
            power: validator.power,
        }
        .abi_encode();

        let tx = TxEnv::builder()
            .caller(owner)
            .nonce(owner_nonce as u64)
            .gas_limit(GENESIS_TX_GAS_LIMIT)
            .call(proxy_address)
            .data(calldata.into())
            .build()
            .map_err(|e| eyre!("failed to build register CALL tx: {e:?}"))?;

        let result = evm
            .transact_commit(tx)
            .map_err(|e| eyre!("register CALL tx failed: {e:?}"))?;

        if !result.is_success() {
            return Err(eyre!("register CALL tx reverted: {result:?}"));
        }
    }

    let mut alloc = extract_alloc(evm.db_mut());
    alloc.remove(&GENESIS_DEPLOYER);
    if owner != GENESIS_DEPLOYER {
        alloc.remove(&owner);
    }

    Ok(ValidatorManagerGenesisAlloc {
        proxy_address,
        implementation_address,
        alloc,
    })
}

fn validate_validators(validators: &[Validator]) -> Result<()> {
    if validators.is_empty() {
        return Err(eyre!("validator list cannot be empty"));
    }

    let mut seen = HashSet::new();
    for validator in validators {
        if validator.power == 0 {
            return Err(eyre!(
                "validator ({:#x}, {:#x}) has zero power",
                validator.validator_key.0,
                validator.validator_key.1
            ));
        }

        if !seen.insert(validator.validator_key) {
            return Err(eyre!(
                "duplicate validator key ({:#x}, {:#x})",
                validator.validator_key.0,
                validator.validator_key.1
            ));
        }
    }

    Ok(())
}

fn extract_alloc(db: &mut InMemoryDB) -> BTreeMap<Address, GenesisAccount> {
    let mut alloc = BTreeMap::new();

    for (address, account) in &db.cache.accounts {
        let Some(info) = account.info() else {
            continue;
        };

        let mut storage = BTreeMap::new();
        for (slot, value) in &account.storage {
            storage.insert((*slot).into(), (*value).into());
        }

        let code = info.code.as_ref().and_then(|bytecode| {
            let bytes = bytecode.original_bytes();
            (!bytes.is_empty()).then_some(bytes.clone())
        });

        alloc.insert(
            *address,
            GenesisAccount {
                nonce: Some(info.nonce),
                balance: info.balance,
                code,
                storage: (!storage.is_empty()).then_some(storage),
                ..Default::default()
            },
        );
    }

    alloc
}
