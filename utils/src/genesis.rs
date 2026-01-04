use core::str::FromStr;
use std::collections::BTreeMap;

use alloy_genesis::{ChainConfig, Genesis, GenesisAccount};
use alloy_primitives::{address, hex, Address, B256, U256};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use chrono::NaiveDate;
use color_eyre::eyre::{eyre, Result};
use hex::decode;
use k256::ecdsa::VerifyingKey;
// Malachite types for Emerald genesis
use malachitebft_eth_types::secp256k1::PublicKey as EmeraldPublicKey;
use malachitebft_eth_types::{
    Genesis as EmeraldGenesis, Validator as EmeraldValidator, ValidatorSet as EmeraldValidatorSet,
};
use tracing::debug;

use crate::validator_manager::contract::{ValidatorManager, GENESIS_VALIDATOR_MANAGER_ACCOUNT};
use crate::validator_manager::{generate_storage_data, Validator};

/// EIP-4788 Beacon Roots Contract address
const BEACON_ROOTS_ADDRESS: Address = address!("0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02");

// TODO: This should be removed or parametrized per chain when Emerald starts supporting PoS.
// This address is the address of the Etherium smart contract that handles staking.
const ETH_DEPOSITS_CONTRACT_ADDRESS: Address =
    address!("0x00000000219ab540356cbb839cbe05303d7705fa");

/// EIP-4788 Beacon Roots Contract bytecode
/// See: https://eips.ethereum.org/EIPS/eip-4788
const BEACON_ROOTS_CODE: [u8; 97] = hex!("0x3373fffffffffffffffffffffffffffffffffffffffe14604d57602036146024575f5ffd5b5f35801560495762001fff810690815414603c575f5ffd5b62001fff01545f5260205ff35b5f5ffd5b62001fff42064281555f359062001fff015500");

const WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS: Address =
    address!("0x00000961ef480eb55e80d19ad83579a64c007002");

const WITHDRAWAL_REQUEST_PREDEPLOY_CODE : [u8; 504] = hex!("0x3373fffffffffffffffffffffffffffffffffffffffe1460cb5760115f54807fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff146101f457600182026001905f5b5f82111560685781019083028483029004916001019190604d565b909390049250505036603814608857366101f457346101f4575f5260205ff35b34106101f457600154600101600155600354806003026004013381556001015f35815560010160203590553360601b5f5260385f601437604c5fa0600101600355005b6003546002548082038060101160df575060105b5f5b8181146101835782810160030260040181604c02815460601b8152601401816001015481526020019060020154807fffffffffffffffffffffffffffffffff00000000000000000000000000000000168252906010019060401c908160381c81600701538160301c81600601538160281c81600501538160201c81600401538160181c81600301538160101c81600201538160081c81600101535360010160e1565b910180921461019557906002556101a0565b90505f6002555f6003555b5f54807fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff14156101cd57505f5b6001546002828201116101e25750505f6101e8565b01600290035b5f555f600155604c025ff35b5f5ffd");

const CONSOLIDATION_REQUEST_PREDEPLOY_ADDRESS: Address =
    address!("0x0000bbddc7ce488642fb579f8b00f3a590007251");

const CONSOLIDATION_REQUEST_PREDEPLOY_CODE: [u8;414] = hex!("0x3373fffffffffffffffffffffffffffffffffffffffe1460d35760115f54807fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1461019a57600182026001905f5b5f82111560685781019083028483029004916001019190604d565b9093900492505050366060146088573661019a573461019a575f5260205ff35b341061019a57600154600101600155600354806004026004013381556001015f358155600101602035815560010160403590553360601b5f5260605f60143760745fa0600101600355005b6003546002548082038060021160e7575060025b5f5b8181146101295782810160040260040181607402815460601b815260140181600101548152602001816002015481526020019060030154905260010160e9565b910180921461013b5790600255610146565b90505f6002555f6003555b5f54807fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff141561017357505f5b6001546001828201116101885750505f61018e565b01600190035b5f555f6001556074025ff35b5f5ffd");

const DEPOSIT_CONTRACT_ADDRESS: Address = address!("0x00000000219ab540356cbb839cbe05303d7705fa");

const DEPOSIT_CONTRACT_CODE: [u8; 6358] = hex!("0x60806040526004361061003f5760003560e01c806301ffc9a71461004457806322895118146100a4578063621fd130146101ba578063c5f2892f14610244575b600080fd5b34801561005057600080fd5b506100906004803603602081101561006757600080fd5b50357fffffffff000000000000000000000000000000000000000000000000000000001661026b565b604080519115158252519081900360200190f35b6101b8600480360360808110156100ba57600080fd5b8101906020810181356401000000008111156100d557600080fd5b8201836020820111156100e757600080fd5b8035906020019184600183028401116401000000008311171561010957600080fd5b91939092909160208101903564010000000081111561012757600080fd5b82018360208201111561013957600080fd5b8035906020019184600183028401116401000000008311171561015b57600080fd5b91939092909160208101903564010000000081111561017957600080fd5b82018360208201111561018b57600080fd5b803590602001918460018302840111640100000000831117156101ad57600080fd5b919350915035610304565b005b3480156101c657600080fd5b506101cf6110b5565b6040805160208082528351818301528351919283929083019185019080838360005b838110156102095781810151838201526020016101f1565b50505050905090810190601f1680156102365780820380516001836020036101000a031916815260200191505b509250505060405180910390f35b34801561025057600080fd5b506102596110c7565b60408051918252519081900360200190f35b60007fffffffff0000000000000000000000000000000000000000000000000000000082167f01ffc9a70000000000000000000000000000000000000000000000000000000014806102fe57507fffffffff0000000000000000000000000000000000000000000000000000000082167f8564090700000000000000000000000000000000000000000000000000000000145b92915050565b6030861461035d576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260268152602001806118056026913960400191505060405180910390fd5b602084146103b6576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252603681526020018061179c6036913960400191505060405180910390fd5b6060821461040f576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260298152602001806118786029913960400191505060405180910390fd5b670de0b6b3a7640000341015610470576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260268152602001806118526026913960400191505060405180910390fd5b633b9aca003406156104cd576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260338152602001806117d26033913960400191505060405180910390fd5b633b9aca00340467ffffffffffffffff811115610535576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252602781526020018061182b6027913960400191505060405180910390fd5b6060610540826114ba565b90507f649bbc62d0e31342afea4e5cd82d4049e7e1ee912fc0889aa790803be39038c589898989858a8a6105756020546114ba565b6040805160a0808252810189905290819060208201908201606083016080840160c085018e8e80828437600083820152601f017fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe01690910187810386528c815260200190508c8c808284376000838201819052601f9091017fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe01690920188810386528c5181528c51602091820193918e019250908190849084905b83811015610648578181015183820152602001610630565b50505050905090810190601f1680156106755780820380516001836020036101000a031916815260200191505b5086810383528881526020018989808284376000838201819052601f9091017fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0169092018881038452895181528951602091820193918b019250908190849084905b838110156106ef5781810151838201526020016106d7565b50505050905090810190601f16801561071c5780820380516001836020036101000a031916815260200191505b509d505050505050505050505050505060405180910390a1600060028a8a600060801b604051602001808484808284377fffffffffffffffffffffffffffffffff0000000000000000000000000000000090941691909301908152604080517ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0818403018152601090920190819052815191955093508392506020850191508083835b602083106107fc57805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe090920191602091820191016107bf565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa158015610859573d6000803e3d6000fd5b5050506040513d602081101561086e57600080fd5b5051905060006002806108846040848a8c6116fe565b6040516020018083838082843780830192505050925050506040516020818303038152906040526040518082805190602001908083835b602083106108f857805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe090920191602091820191016108bb565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa158015610955573d6000803e3d6000fd5b5050506040513d602081101561096a57600080fd5b5051600261097b896040818d6116fe565b60405160009060200180848480828437919091019283525050604080518083038152602092830191829052805190945090925082918401908083835b602083106109f457805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe090920191602091820191016109b7565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa158015610a51573d6000803e3d6000fd5b5050506040513d6020811015610a6657600080fd5b5051604080516020818101949094528082019290925280518083038201815260609092019081905281519192909182918401908083835b60208310610ada57805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe09092019160209182019101610a9d565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa158015610b37573d6000803e3d6000fd5b5050506040513d6020811015610b4c57600080fd5b50516040805160208101858152929350600092600292839287928f928f92018383808284378083019250505093505050506040516020818303038152906040526040518082805190602001908083835b60208310610bd957805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe09092019160209182019101610b9c565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa158015610c36573d6000803e3d6000fd5b5050506040513d6020811015610c4b57600080fd5b50516040518651600291889160009188916020918201918291908601908083835b60208310610ca957805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe09092019160209182019101610c6c565b6001836020036101000a0380198251168184511680821785525050505050509050018367ffffffffffffffff191667ffffffffffffffff1916815260180182815260200193505050506040516020818303038152906040526040518082805190602001908083835b60208310610d4e57805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe09092019160209182019101610d11565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa158015610dab573d6000803e3d6000fd5b5050506040513d6020811015610dc057600080fd5b5051604080516020818101949094528082019290925280518083038201815260609092019081905281519192909182918401908083835b60208310610e3457805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe09092019160209182019101610df7565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa158015610e91573d6000803e3d6000fd5b5050506040513d6020811015610ea657600080fd5b50519050858114610f02576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260548152602001806117486054913960600191505060405180910390fd5b60205463ffffffff11610f60576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260218152602001806117276021913960400191505060405180910390fd5b602080546001019081905560005b60208110156110a9578160011660011415610fa0578260008260208110610f9157fe5b0155506110ac95505050505050565b600260008260208110610faf57fe5b01548460405160200180838152602001828152602001925050506040516020818303038152906040526040518082805190602001908083835b6020831061102557805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe09092019160209182019101610fe8565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa158015611082573d6000803e3d6000fd5b5050506040513d602081101561109757600080fd5b50519250600282049150600101610f6e565b50fe5b50505050505050565b60606110c26020546114ba565b905090565b6020546000908190815b60208110156112f05781600116600114156111e6576002600082602081106110f557fe5b01548460405160200180838152602001828152602001925050506040516020818303038152906040526040518082805190602001908083835b6020831061116b57805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0909201916020918201910161112e565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa1580156111c8573d6000803e3d6000fd5b5050506040513d60208110156111dd57600080fd5b505192506112e2565b600283602183602081106111f657fe5b015460405160200180838152602001828152602001925050506040516020818303038152906040526040518082805190602001908083835b6020831061126b57805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0909201916020918201910161122e565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa1580156112c8573d6000803e3d6000fd5b5050506040513d60208110156112dd57600080fd5b505192505b6002820491506001016110d1565b506002826112ff6020546114ba565b600060401b6040516020018084815260200183805190602001908083835b6020831061135a57805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0909201916020918201910161131d565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790527fffffffffffffffffffffffffffffffffffffffffffffffff000000000000000095909516920191825250604080518083037ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8018152601890920190819052815191955093508392850191508083835b6020831061143f57805182527fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe09092019160209182019101611402565b51815160209384036101000a7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff01801990921691161790526040519190930194509192505080830381855afa15801561149c573d6000803e3d6000fd5b5050506040513d60208110156114b157600080fd5b50519250505090565b60408051600880825281830190925260609160208201818036833701905050905060c082901b8060071a60f81b826000815181106114f457fe5b60200101907effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916908160001a9053508060061a60f81b8260018151811061153757fe5b60200101907effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916908160001a9053508060051a60f81b8260028151811061157a57fe5b60200101907effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916908160001a9053508060041a60f81b826003815181106115bd57fe5b60200101907effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916908160001a9053508060031a60f81b8260048151811061160057fe5b60200101907effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916908160001a9053508060021a60f81b8260058151811061164357fe5b60200101907effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916908160001a9053508060011a60f81b8260068151811061168657fe5b60200101907effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916908160001a9053508060001a60f81b826007815181106116c957fe5b60200101907effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1916908160001a90535050919050565b6000808585111561170d578182fd5b83861115611719578182fd5b505082019391909203915056fe4465706f736974436f6e74726163743a206d65726b6c6520747265652066756c6c4465706f736974436f6e74726163743a207265636f6e7374727563746564204465706f7369744461746120646f6573206e6f74206d6174636820737570706c696564206465706f7369745f646174615f726f6f744465706f736974436f6e74726163743a20696e76616c6964207769746864726177616c5f63726564656e7469616c73206c656e6774684465706f736974436f6e74726163743a206465706f7369742076616c7565206e6f74206d756c7469706c65206f6620677765694465706f736974436f6e74726163743a20696e76616c6964207075626b6579206c656e6774684465706f736974436f6e74726163743a206465706f7369742076616c756520746f6f20686967684465706f736974436f6e74726163743a206465706f7369742076616c756520746f6f206c6f774465706f736974436f6e74726163743a20696e76616c6964207369676e6174757265206c656e677468a2646970667358221220dceca8706b29e917dacf25fceef95acac8d90d765ac926663ce4096195952b6164736f6c634300060b0033");

/// Test mnemonic for wallet generation
const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

/// Create a signer from a mnemonic.
pub(crate) fn make_signer(index: u64) -> PrivateKeySigner {
    MnemonicBuilder::<English>::default()
        .phrase(TEST_MNEMONIC)
        .derivation_path(format!("m/44'/60'/0'/0/{index}"))
        .expect("Failed to set derivation path")
        .build()
        .expect("Failed to create wallet")
}

pub(crate) fn make_signers() -> Vec<PrivateKeySigner> {
    (0..10).map(make_signer).collect()
}

pub(crate) fn generate_genesis(
    public_keys_file: &str,
    poa_address_owner: &Option<String>,
    testnet: &bool,
    testnet_balance: &u64,
    chain_id: &u64,
    evm_genesis_output_file: &str,
    emerald_genesis_output_file: &str,
) -> Result<()> {
    generate_evm_genesis(
        public_keys_file,
        poa_address_owner,
        testnet,
        testnet_balance,
        chain_id,
        evm_genesis_output_file,
    )?;

    generate_emerald_genesis(public_keys_file, emerald_genesis_output_file)?;

    Ok(())
}

pub(crate) fn generate_evm_genesis(
    public_keys_file: &str,
    poa_address_owner: &Option<String>,
    testnet: &bool,
    testnet_balance: &u64,
    chain_id: &u64,
    genesis_output_file: &str,
) -> Result<()> {
    let mut alloc = BTreeMap::new();
    let signers = make_signers();
    // If test addresses are requested, create them and pre-fund them
    if *testnet {
        // Create signers and get their addresses
        let signer_addresses: Vec<Address> =
            signers.iter().map(|signer| signer.address()).collect();

        debug!("Using signer addresses:");
        for (i, (signer, addr)) in signers.iter().zip(signer_addresses.iter()).enumerate() {
            debug!(
                "Signer {i}: {addr} ({})",
                B256::from_slice(&signer.credential().to_bytes())
            );
        }

        let amount = U256::from(*testnet_balance) * U256::from(10).pow(U256::from(18));
        for addr in &signer_addresses {
            alloc.insert(
                *addr,
                GenesisAccount {
                    balance: amount,
                    ..Default::default()
                },
            );
        }
    }

    let mut initial_validators = Vec::new();
    for (idx, raw_line) in std::fs::read_to_string(public_keys_file)?
        .lines()
        .enumerate()
    {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let hex_str = line.strip_prefix("0x").unwrap_or(line);
        let bytes = decode(hex_str).map_err(|e| {
            eyre!(
                "invalid hex-encoded validator key at line {} in {}: {}",
                idx + 1,
                public_keys_file,
                e
            )
        })?;

        if bytes.len() != 64 {
            return Err(eyre!(
                "expected 64-byte uncompressed secp256k1 payload (sans 0x04 prefix) at line {} in {}, got {} bytes",
                idx + 1,
                public_keys_file,
                bytes.len()
            ));
        }

        let mut uncompressed = [0u8; 65];
        uncompressed[0] = 0x04;
        uncompressed[1..].copy_from_slice(&bytes);

        VerifyingKey::from_sec1_bytes(&uncompressed).map_err(|_| {
            eyre!(
                "invalid secp256k1 public key material at line {} in {}",
                idx + 1,
                public_keys_file
            )
        })?;

        let mut x_bytes = [0u8; 32];
        x_bytes.copy_from_slice(&bytes[..32]);
        let mut y_bytes = [0u8; 32];
        y_bytes.copy_from_slice(&bytes[32..]);
        let key = (U256::from_be_bytes(x_bytes), U256::from_be_bytes(y_bytes));
        initial_validators.push(Validator::from_public_key(key, 100));
    }

    // Parse PoA owner address or override with first test address
    let poa_address_owner = if let Some(addr_str) = poa_address_owner {
        Address::from_str(addr_str)
            .map_err(|e| eyre!("invalid PoA owner address '{}': {}", addr_str, e))?
    } else if *testnet {
        signers[0].address()
    } else {
        unreachable!("unable to determine PoA owner address");
    };

    let storage = generate_storage_data(initial_validators, poa_address_owner)?;
    alloc.insert(
        GENESIS_VALIDATOR_MANAGER_ACCOUNT,
        GenesisAccount {
            code: Some(ValidatorManager::DEPLOYED_BYTECODE.clone()),
            storage: Some(storage),
            ..Default::default()
        },
    );

    // Deploy EIP-4788 Beacon Roots Contract
    // Required for Engine API V3 compliance when parent_beacon_block_root is set
    // reth deploys this contract at genesis but only for chain-id 1 so we add it here manually in
    // order to support arbitrary chain-ids
    alloc.insert(
        BEACON_ROOTS_ADDRESS,
        GenesisAccount {
            code: Some(BEACON_ROOTS_CODE.into()),
            ..Default::default()
        },
    );

    alloc.insert(
        WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
        GenesisAccount {
            code: Some(WITHDRAWAL_REQUEST_PREDEPLOY_CODE.into()),
            balance: U256::from(100),
            ..Default::default()
        },
    );

    alloc.insert(
        CONSOLIDATION_REQUEST_PREDEPLOY_ADDRESS,
        GenesisAccount {
            code: Some(CONSOLIDATION_REQUEST_PREDEPLOY_CODE.into()),
            balance: U256::from(100),
            ..Default::default()
        },
    );

    alloc.insert(
        DEPOSIT_CONTRACT_ADDRESS,
        GenesisAccount {
            code: Some(DEPOSIT_CONTRACT_CODE.into()),
            balance: U256::from(100),
            ..Default::default()
        },
    );

    // The Ethereum Prague-Electra (Pectra) upgrade was activated on the mainnet
    // on May 7, 2025, at epoch 364,032.
    let date = NaiveDate::from_ymd_opt(2025, 5, 7).expect("Failed to create date for May 7, 2025");
    let datetime = date
        .and_hms_opt(0, 0, 0)
        .expect("Failed to create datetime with 00:00:00");
    let valid_pectra_timestamp = datetime.and_utc().timestamp() as u64;

    // Create genesis configuration
    let genesis = Genesis {
        config: ChainConfig {
            chain_id: *chain_id,
            homestead_block: Some(0),
            eip150_block: Some(0),
            eip155_block: Some(0),
            eip158_block: Some(0),
            byzantium_block: Some(0),
            constantinople_block: Some(0),
            petersburg_block: Some(0),
            istanbul_block: Some(0),
            berlin_block: Some(0),
            london_block: Some(0),
            shanghai_time: Some(0),
            cancun_time: Some(0),
            prague_time: Some(0),
            // osaka_time: Some(0),
            terminal_total_difficulty: Some(U256::ZERO),
            terminal_total_difficulty_passed: true,
            // This was added only because Ethrex requires this to exist
            // TODO remove until Emerald supports PoS
            deposit_contract_address: Some(ETH_DEPOSITS_CONTRACT_ADDRESS),
            ..Default::default()
        },
        alloc,
        ..Default::default()
    }
    .with_gas_limit(30_000_000)
    .with_timestamp(valid_pectra_timestamp);

    // Create data directory if it doesn't exist
    std::fs::create_dir_all("./assets")?;

    // Write genesis to file
    let genesis_json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(genesis_output_file, genesis_json)?;
    debug!("Genesis configuration written to {genesis_output_file}");

    Ok(())
}

/// Generate Malachite/Emerald genesis file from validator public keys
pub(crate) fn generate_emerald_genesis(
    public_keys_file: &str,
    emerald_genesis_output_file: &str,
) -> Result<()> {
    debug!("Generating Emerald genesis file from {public_keys_file}");

    let mut validators = Vec::new();

    for (idx, raw_line) in std::fs::read_to_string(public_keys_file)?
        .lines()
        .enumerate()
    {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse hex-encoded public key (64 bytes without 0x04 prefix)
        let hex_str = line.strip_prefix("0x").unwrap_or(line);
        let bytes = decode(hex_str).map_err(|e| {
            eyre!(
                "invalid hex-encoded validator key at line {} in {}: {}",
                idx + 1,
                public_keys_file,
                e
            )
        })?;

        if bytes.len() != 64 {
            return Err(eyre!(
                "expected 64-byte uncompressed secp256k1 payload (sans 0x04 prefix) at line {} in {}, got {} bytes",
                idx + 1,
                public_keys_file,
                bytes.len()
            ));
        }

        // Convert to uncompressed SEC1 format (65 bytes with 0x04 prefix)
        let mut uncompressed = [0u8; 65];
        uncompressed[0] = 0x04;
        uncompressed[1..].copy_from_slice(&bytes);

        // Validate and create public key
        let pub_key = EmeraldPublicKey::from_sec1_bytes(&uncompressed).map_err(|_| {
            eyre!(
                "invalid secp256k1 public key material at line {} in {}",
                idx + 1,
                public_keys_file
            )
        })?;

        // Create validator with voting power of 1
        validators.push(EmeraldValidator::new(pub_key, 1));
    }

    if validators.is_empty() {
        return Err(eyre!("no valid validators found in {}", public_keys_file));
    }

    // Create validator set and genesis
    let validator_set = EmeraldValidatorSet::new(validators);
    let genesis = EmeraldGenesis { validator_set };

    // Write emerald genesis to file
    let genesis_json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(emerald_genesis_output_file, genesis_json)?;
    debug!("Emerald genesis configuration written to {emerald_genesis_output_file}");

    Ok(())
}
