use alloy_dyn_abi::{DynSolValue, JsonAbiExt, Word};
use alloy_json_abi::Function;
use alloy_primitives::{Address, U256};
use color_eyre::eyre::Result;

use crate::dex_templates::{Eip1559Template, TxTemplate};

/// Dexalot contract addresses
pub struct DexalotConfig {
    pub portfolio: Address,
    pub tradepairs: Address,
}

/// Generate Dexalot trading transactions dynamically for a given trader address
pub fn generate_dexalot_transactions(
    trader: Address,
    config: DexalotConfig,
) -> Result<Vec<TxTemplate>> {
    let mut transactions = Vec::new();

    // 1. Deposit transaction
    let deposit_calldata = encode_deposit_native(trader)?;
    transactions.push(TxTemplate::Eip1559(Eip1559Template {
        to: format!("{:#x}", config.portfolio),
        value: "0.1".to_string(),
        gas_limit: 200000,
        max_fee_per_gas: "5".to_string(),
        max_priority_fee_per_gas: "2".to_string(),
        input: format!("0x{}", hex::encode(deposit_calldata)),
    }));

    // Trading pair configuration
    let tradepair_id =
        hex::decode("415641582f555344540000000000000000000000000000000000000000000000")?;
    let base_price = 15.0; // 15 USDT
    let price_increment = 0.1;

    // Varying sell quantities to create partial fills
    let sell_quantities = vec![
        U256::from(1_000_000_000_000_000_000u128), // 1.0 AVAX
        U256::from(1_500_000_000_000_000_000u128), // 1.5 AVAX
        U256::from(800_000_000_000_000_000u128),   // 0.8 AVAX
        U256::from(1_200_000_000_000_000_000u128), // 1.2 AVAX
        U256::from(900_000_000_000_000_000u128),   // 0.9 AVAX
        U256::from(1_100_000_000_000_000_000u128), // 1.1 AVAX
        U256::from(700_000_000_000_000_000u128),   // 0.7 AVAX
        U256::from(1_300_000_000_000_000_000u128), // 1.3 AVAX
        U256::from(950_000_000_000_000_000u128),   // 0.95 AVAX
        U256::from(1_050_000_000_000_000_000u128), // 1.05 AVAX
    ];

    // Varying buy quantities to create partial fills
    let buy_quantities = vec![
        U256::from(600_000_000_000_000_000u128), // 0.6 AVAX
        U256::from(750_000_000_000_000_000u128), // 0.75 AVAX
        U256::from(500_000_000_000_000_000u128), // 0.5 AVAX
        U256::from(850_000_000_000_000_000u128), // 0.85 AVAX
        U256::from(650_000_000_000_000_000u128), // 0.65 AVAX
        U256::from(550_000_000_000_000_000u128), // 0.55 AVAX
        U256::from(700_000_000_000_000_000u128), // 0.7 AVAX
        U256::from(800_000_000_000_000_000u128), // 0.8 AVAX
        U256::from(450_000_000_000_000_000u128), // 0.45 AVAX
        U256::from(900_000_000_000_000_000u128), // 0.9 AVAX
    ];

    // 2-11. Generate 10 SELL orders
    for i in 0..10 {
        let client_id = generate_client_order_id(i);
        let price = base_price - 0.5 + (i as f64) * price_increment;
        let price_wei = U256::from((price * 1e18) as u128);

        let order_calldata = encode_add_order_list(
            &client_id,
            &tradepair_id,
            price_wei,
            sell_quantities[i as usize],
            trader,
            1,
        )?;

        transactions.push(TxTemplate::Eip1559(Eip1559Template {
            to: format!("{:#x}", config.tradepairs),
            value: "0.0".to_string(),
            gas_limit: 500000,
            max_fee_per_gas: "5".to_string(),
            max_priority_fee_per_gas: "2".to_string(),
            input: format!("0x{}", hex::encode(order_calldata)),
        }));
    }

    // 12-21. Generate 10 BUY orders
    for i in 0..10 {
        let client_id = generate_client_order_id(100 + i);
        let price = base_price + (i as f64) * price_increment;
        let price_wei = U256::from((price * 1e18) as u128);

        let order_calldata = encode_add_order_list(
            &client_id,
            &tradepair_id,
            price_wei,
            buy_quantities[i as usize],
            trader,
            0,
        )?;

        transactions.push(TxTemplate::Eip1559(Eip1559Template {
            to: format!("{:#x}", config.tradepairs),
            value: "0.0".to_string(),
            gas_limit: 500000,
            max_fee_per_gas: "5".to_string(),
            max_priority_fee_per_gas: "2".to_string(),
            input: format!("0x{}", hex::encode(order_calldata)),
        }));
    }

    Ok(transactions)
}

/// Encode depositNative(address,uint8) function call
fn encode_deposit_native(trader: Address) -> Result<Vec<u8>> {
    let function = Function::parse("depositNative(address,uint8)")?;
    let args = vec![
        DynSolValue::Address(trader),
        DynSolValue::Uint(U256::from(0), 8),
    ];
    Ok(function.abi_encode_input(&args)?)
}

/// Encode addOrderList function call
fn encode_add_order_list(
    client_order_id: &[u8],
    tradepair_id: &[u8],
    price: U256,
    quantity: U256,
    trader: Address,
    side: u8, // 0 = BUY, 1 = SELL
) -> Result<Vec<u8>> {
    let function = Function::parse(
        "addOrderList((bytes32,bytes32,uint256,uint256,address,uint8,uint8,uint8,uint8)[])",
    )?;

    // Build the order tuple
    let client_order_id_word = Word::try_from(client_order_id)
        .map_err(|_| color_eyre::eyre::eyre!("Invalid client order ID length"))?;
    let tradepair_id_word = Word::try_from(tradepair_id)
        .map_err(|_| color_eyre::eyre::eyre!("Invalid tradepair ID length"))?;

    let order_tuple = DynSolValue::Tuple(vec![
        DynSolValue::FixedBytes(Word::from(client_order_id_word), 32),
        DynSolValue::FixedBytes(Word::from(tradepair_id_word), 32),
        DynSolValue::Uint(price, 256),
        DynSolValue::Uint(quantity, 256),
        DynSolValue::Address(trader),
        DynSolValue::Uint(U256::from(side), 8), // side
        DynSolValue::Uint(U256::from(1), 8),    // type1 = LIMIT
        DynSolValue::Uint(U256::from(0), 8),    // type2 = GTC
        DynSolValue::Uint(U256::from(3), 8),    // stp = NONE
    ]);

    let orders_array = DynSolValue::Array(vec![order_tuple]);
    let args = vec![orders_array];

    Ok(function.abi_encode_input(&args)?)
}

/// Generate unique client order ID using timestamp
fn generate_client_order_id(num: u32) -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let unique_id = (timestamp * 1000) + num as u64;

    let mut bytes = vec![0u8; 32];
    let id_bytes = unique_id.to_be_bytes();
    bytes[24..32].copy_from_slice(&id_bytes);
    bytes
}
