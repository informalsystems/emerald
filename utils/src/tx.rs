use alloy_consensus::{SignableTransaction, TxEip1559, TxEip4844};
use alloy_dyn_abi::{JsonAbiExt, Specifier};
use alloy_json_abi::Function;
use alloy_primitives::{Address, Bytes, B256, U256};
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::{bail, Result};
use reth_primitives::{Transaction, TransactionSigned};

pub(crate) fn make_eip4844_tx(nonce: u64) -> Transaction {
    Transaction::Eip4844(TxEip4844 {
        chain_id: 1u64,
        nonce,
        max_fee_per_gas: 50_000_000_000,             // 50 gwei
        max_priority_fee_per_gas: 1_000_000_000_000, // 1000 gwei
        gas_limit: 21_000,
        to: Address::left_padding_from(&[5]),
        value: U256::from(10e15), // 0.001 ETH
        input: Bytes::default(),
        access_list: Default::default(),
        blob_versioned_hashes: vec![B256::random()],
        max_fee_per_blob_gas: 0,
    })
}

async fn sign_transaction(signer: &PrivateKeySigner, tx: Transaction) -> Result<TransactionSigned> {
    let tx_sign_hash = tx.signature_hash();
    let signature = signer.sign_hash(&tx_sign_hash).await?;
    Ok(TransactionSigned::new_unhashed(tx, signature))
}

pub(crate) async fn make_signed_eip4844_tx(
    signer: &PrivateKeySigner,
    nonce: u64,
) -> Result<TransactionSigned> {
    let tx = make_eip4844_tx(nonce);
    sign_transaction(signer, tx).await
}

pub(crate) fn make_eip1559_tx(nonce: u64) -> Transaction {
    Transaction::Eip1559(TxEip1559 {
        chain_id: 1u64,
        nonce,
        max_priority_fee_per_gas: 1_000_000_000, // 1 gwei
        max_fee_per_gas: 2_000_000_000,          // 2 gwei
        gas_limit: 50_000,
        to: Address::left_padding_from(&[5]).into(),
        value: U256::from(10), // 10 wei
        input: Bytes::default(),
        access_list: Default::default(),
    })
}

pub(crate) async fn make_signed_eip1559_tx(
    signer: &PrivateKeySigner,
    nonce: u64,
) -> Result<TransactionSigned> {
    let tx = make_eip1559_tx(nonce);
    sign_transaction(signer, tx).await
}

pub(crate) fn make_contract_call_tx(
    nonce: u64,
    contract_address: Address,
    function_sig: &str,
    args: &[String],
) -> Result<Transaction> {
    let function = Function::parse(function_sig)?;
    if function.inputs.len() != args.len() {
        bail!(
            "argument count mismatch for `{}`: expected {}, got {}",
            function_sig,
            function.inputs.len(),
            args.len()
        );
    }
    let arg_values = function
        .inputs
        .iter()
        .zip(args.iter())
        .map(|(param, value)| {
            let param_ty = param.resolve()?;
            let coerced_value = param_ty.coerce_str(value)?;
            Ok(coerced_value)
        })
        .collect::<Result<Vec<_>>>()?;
    let call_data = function.abi_encode_input(&arg_values)?;

    Ok(Transaction::Eip1559(TxEip1559 {
        chain_id: 1u64,
        nonce,
        max_priority_fee_per_gas: 1_000_000_000, // 1 gwei
        max_fee_per_gas: 2_000_000_000,          // 2 gwei
        gas_limit: 100_000,                      // Higher gas limit for contract calls
        to: contract_address.into(),
        value: U256::ZERO, // No ETH transfer
        input: call_data.into(),
        access_list: Default::default(),
    }))
}

pub(crate) async fn make_signed_contract_call_tx(
    signer: &PrivateKeySigner,
    nonce: u64,
    contract_address: Address,
    function_sig: &str,
    args: &[String],
) -> Result<TransactionSigned> {
    let tx = make_contract_call_tx(nonce, contract_address, function_sig, args)?;
    sign_transaction(signer, tx).await
}

#[cfg(test)]
mod tests {
    use alloy_network::eip2718::Encodable2718;
    use alloy_primitives::PrimitiveSignature as Signature;
    use alloy_rlp::Decodable;

    use super::*;

    #[tokio::test]
    async fn test_encode_decode_signed_eip4844_tx() {
        let tx = make_eip4844_tx(0);
        let signature = Signature::test_signature();
        let signed_tx = TransactionSigned::new_unhashed(tx, signature);
        let tx_bytes = signed_tx.encoded_2718();

        let decoded_signed_tx = TransactionSigned::decode(&mut tx_bytes.as_slice()).unwrap();
        assert_eq!(decoded_signed_tx, signed_tx);
    }

    #[tokio::test]
    async fn test_encode_decode_signed_eip1559_tx() {
        let tx = make_eip1559_tx(0);
        let signature = Signature::test_signature();
        let signed_tx = TransactionSigned::new_unhashed(tx, signature);
        let tx_bytes = signed_tx.encoded_2718();

        let decoded_signed_tx = TransactionSigned::decode(&mut tx_bytes.as_slice()).unwrap();
        assert_eq!(decoded_signed_tx, signed_tx);
    }

    // #[test]
    // fn test_eth_pooled_transaction_new_eip4844() {
    //      use alloy_consensus::Transaction;
    //      use alloy_eips::eip4844::DATA_GAS_PER_BLOB;
    // use reth_primitives::Recovered;
    // use reth_transaction_pool::{EthBlobTransactionSidecar, EthPooledTransaction};
    //     // Create an EIP-4844 transaction
    //     let tx = make_eip4844_tx(0);
    //     let signature = Signature::test_signature();
    //     let signed_tx = TransactionSigned::new_unhashed(tx.clone(), signature);
    //     let transaction = Recovered::new_unchecked(signed_tx, Default::default());

    //     let mut blobs_bundle = self
    //         .blobs_bundle_path
    //         .map(|path| -> eyre::Result<BlobsBundleV1> {
    //             let contents = fs::read_to_string(&path)
    //                 .wrap_err(format!("could not read {}", path.display()))?;
    //             serde_json::from_str(&contents).wrap_err("failed to deserialize blobs bundle")
    //         })
    //         .transpose()?;

    //     let encoded_length = match transaction.transaction() {
    //         Transaction::Eip4844(TxEip4844 {
    //             blob_versioned_hashes,
    //             ..
    //         }) => {
    //             let blobs_bundle = blobs_bundle.as_mut().ok_or_else(|| {
    //                 eyre::eyre!("encountered a blob tx. `--blobs-bundle-path` must be provided")
    //             })?;

    //             let sidecar: BlobTransactionSidecar =
    //                 blobs_bundle.pop_sidecar(blob_versioned_hashes.len());

    //             let pooled = transaction
    //                 .clone()
    //                 .into_tx()
    //                 .try_into_pooled_eip4844(sidecar.clone())
    //                 .expect("should not fail to convert blob tx if it is already eip4844");
    //             let encoded_length = pooled.encode_2718_len();

    //             // insert the blob into the store
    //             blob_store.insert(*transaction.tx_hash(), sidecar)?;

    //             encoded_length
    //         }
    //         _ => transaction.encode_2718_len(),
    //     };

    //     // let pooled_tx = EthPooledTransaction::new(transaction.clone(), 300);
    //     let pooled_tx = EthPooledTransaction::new(transaction, encoded_length);

    //     // Check that the pooled transaction is created correctly
    //     assert_eq!(pooled_tx.transaction, transaction);
    //     assert_eq!(pooled_tx.encoded_length, 300);
    //     assert_eq!(pooled_tx.blob_sidecar, EthBlobTransactionSidecar::Missing);
    //     let expected_cost = tx.value()
    //         + U256::from(tx.max_fee_per_gas()) * U256::from(tx.gas_limit())
    //         + U256::from(tx.max_fee_per_blob_gas().unwrap()) * U256::from(DATA_GAS_PER_BLOB);
    //     assert_eq!(pooled_tx.cost, expected_cost);
    // }
}
