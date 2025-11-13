use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use alloy_primitives::{Address, U256};
use color_eyre::eyre::Result;

use crate::dex_templates::{Eip1559Template, TxTemplate};

/// Rubicon contract addresses
pub struct RubiconConfig {
    pub rubicon_market: Address,
    pub weth: Address,
    pub usdc: Address,
}

/// Generate Rubicon trading transactions dynamically
pub fn generate_rubicon_transactions(config: RubiconConfig) -> Result<Vec<TxTemplate>> {
    let mut transactions = Vec::new();

    // 1. Approve RubiconMarket to spend unlimited WETH
    let weth_approve_calldata = encode_approve(config.rubicon_market, U256::MAX)?;
    transactions.push(TxTemplate::Eip1559(Eip1559Template {
        to: format!("{:#x}", config.weth),
        value: "0.0".to_string(),
        gas_limit: 100000,
        max_fee_per_gas: "2".to_string(),
        max_priority_fee_per_gas: "1".to_string(),
        input: format!("0x{}", hex::encode(weth_approve_calldata)),
    }));

    // 2. Approve RubiconMarket to spend unlimited USDC
    let usdc_approve_calldata = encode_approve(config.rubicon_market, U256::MAX)?;
    transactions.push(TxTemplate::Eip1559(Eip1559Template {
        to: format!("{:#x}", config.usdc),
        value: "0.0".to_string(),
        gas_limit: 100000,
        max_fee_per_gas: "2".to_string(),
        max_priority_fee_per_gas: "1".to_string(),
        input: format!("0x{}", hex::encode(usdc_approve_calldata)),
    }));

    // 3. Deposit 0.001 ETH to get WETH
    let deposit_calldata = encode_deposit()?;
    transactions.push(TxTemplate::Eip1559(Eip1559Template {
        to: format!("{:#x}", config.weth),
        value: "0.001".to_string(),
        gas_limit: 100000,
        max_fee_per_gas: "2".to_string(),
        max_priority_fee_per_gas: "1".to_string(),
        input: format!("0x{}", hex::encode(deposit_calldata)),
    }));

    // 4-13. SELL offers at varying prices (2850-3200 USDC/WETH)
    let sell_prices = vec![3000, 3100, 2950, 3000, 3050, 2900, 3150, 2850, 3200, 2980];
    let weth_amount = U256::from(10_000_000_000_000_000u128); // 0.01 WETH

    for price in sell_prices {
        let usdc_amount = U256::from((price as u128) * 1_000_000_000_000_000_000u128); // price in USDC (18 decimals)
        let offer_calldata =
            encode_offer(weth_amount, config.weth, usdc_amount, config.usdc, 0, true)?;

        transactions.push(TxTemplate::Eip1559(Eip1559Template {
            to: format!("{:#x}", config.rubicon_market),
            value: "0.0".to_string(),
            gas_limit: 500000,
            max_fee_per_gas: "2".to_string(),
            max_priority_fee_per_gas: "1".to_string(),
            input: format!("0x{}", hex::encode(offer_calldata)),
        }));
    }

    // 14. Deposit 1 ETH to get more WETH
    let deposit_calldata_2 = encode_deposit()?;
    transactions.push(TxTemplate::Eip1559(Eip1559Template {
        to: format!("{:#x}", config.weth),
        value: "1".to_string(),
        gas_limit: 100000,
        max_fee_per_gas: "2".to_string(),
        max_priority_fee_per_gas: "1".to_string(),
        input: format!("0x{}", hex::encode(deposit_calldata_2)),
    }));

    // 15. Mint 10,000 USDC using adminMint()
    let admin_mint_calldata = encode_admin_mint()?;
    transactions.push(TxTemplate::Eip1559(Eip1559Template {
        to: format!("{:#x}", config.usdc),
        value: "0.0".to_string(),
        gas_limit: 200000,
        max_fee_per_gas: "2".to_string(),
        max_priority_fee_per_gas: "1".to_string(),
        input: format!("0x{}", hex::encode(admin_mint_calldata)),
    }));

    // 16-23. BUY offers at varying prices (match sells)
    let buy_prices = vec![3000, 3100, 2950, 3000, 3050, 3150, 3200];

    for price in buy_prices {
        let usdc_amount = U256::from((price as u128) * 1_000_000_000_000_000_000u128); // price in USDC
        let offer_calldata =
            encode_offer(usdc_amount, config.usdc, weth_amount, config.weth, 0, true)?;

        transactions.push(TxTemplate::Eip1559(Eip1559Template {
            to: format!("{:#x}", config.rubicon_market),
            value: "0.0".to_string(),
            gas_limit: 500000,
            max_fee_per_gas: "2".to_string(),
            max_priority_fee_per_gas: "1".to_string(),
            input: format!("0x{}", hex::encode(offer_calldata)),
        }));
    }

    Ok(transactions)
}

/// Encode approve(address,uint256) function call
fn encode_approve(spender: Address, amount: U256) -> Result<Vec<u8>> {
    let function = Function::parse("approve(address,uint256)")?;
    let args = vec![
        DynSolValue::Address(spender),
        DynSolValue::Uint(amount, 256),
    ];
    Ok(function.abi_encode_input(&args)?)
}

/// Encode deposit() function call
fn encode_deposit() -> Result<Vec<u8>> {
    let function = Function::parse("deposit()")?;
    Ok(function.abi_encode_input(&[])?)
}

/// Encode adminMint() function call
fn encode_admin_mint() -> Result<Vec<u8>> {
    let function = Function::parse("adminMint()")?;
    Ok(function.abi_encode_input(&[])?)
}

/// Encode offer(uint256,address,uint256,address,uint256,bool) function call
fn encode_offer(
    pay_amt: U256,
    pay_gem: Address,
    buy_amt: U256,
    buy_gem: Address,
    pos: u64,
    matching_enabled: bool,
) -> Result<Vec<u8>> {
    let function = Function::parse("offer(uint256,address,uint256,address,uint256,bool)")?;
    let args = vec![
        DynSolValue::Uint(pay_amt, 256),
        DynSolValue::Address(pay_gem),
        DynSolValue::Uint(buy_amt, 256),
        DynSolValue::Address(buy_gem),
        DynSolValue::Uint(U256::from(pos), 256),
        DynSolValue::Bool(matching_enabled),
    ];
    Ok(function.abi_encode_input(&args)?)
}
