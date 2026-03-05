mod consensus;
mod evm;
use crate::consensus::{EmeraldConsensusBuilder, EmeraldEngineValidatorBuilder};
use crate::evm::EmeraldExecutorBuilder;
use reth_ethereum::cli::interface::Cli;
use reth_ethereum::node::node::{EthereumAddOns, EthereumEthApiBuilder};
use reth_ethereum::node::EthereumNode;
use reth_node_builder::rpc::{BasicEngineApiBuilder, BasicEngineValidatorBuilder, RpcAddOns};

// Custom Reth node with custom timestamp validation for Emerald consensus
fn main() -> eyre::Result<()> {
    Cli::parse_args().run(|builder, _| async move {
        let handle = builder
            .with_types::<EthereumNode>()
            // Use default Ethereum components but override consensus and EVM config
            .with_components(
                EthereumNode::components()
                    .consensus(EmeraldConsensusBuilder)
                    .executor(EmeraldExecutorBuilder),
            )
            .with_add_ons::<EthereumAddOns<
                _,
                EthereumEthApiBuilder,
                EmeraldEngineValidatorBuilder,
                BasicEngineApiBuilder<EmeraldEngineValidatorBuilder>,
                BasicEngineValidatorBuilder<EmeraldEngineValidatorBuilder>,
                _,
            >>(EthereumAddOns::new(RpcAddOns::new(
                EthereumEthApiBuilder::default(),
                EmeraldEngineValidatorBuilder::default(),
                BasicEngineApiBuilder::default(),
                BasicEngineValidatorBuilder::new(EmeraldEngineValidatorBuilder::default()),
                Default::default(),
            )))
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
