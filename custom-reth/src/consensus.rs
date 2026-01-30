//! Custom consensus implementation with relaxed timestamp validation for Emerald

use std::fmt::Debug;
use std::sync::Arc;

use reth_engine_primitives::{EngineApiValidator, PayloadValidator};
use reth_ethereum::chainspec::{ChainSpec, EthChainSpec, EthereumHardforks};
use reth_ethereum::consensus::{
    validation, Consensus, ConsensusError, FullConsensus, HeaderConsensusError, HeaderValidator,
};
use reth_ethereum::engine::EthereumEngineValidator;
use reth_ethereum::provider::BlockExecutionResult;
use reth_ethereum::EthPrimitives;
use reth_node_api::node::FullNodeComponents;
use reth_node_api::{AddOnsContext, FullNodeTypes, NodeTypes, PayloadTypes};
use reth_node_builder::components::ConsensusBuilder;
use reth_node_builder::rpc::PayloadValidatorBuilder;
use reth_node_builder::BuilderContext;
use reth_payload_primitives::{
    EngineApiMessageVersion, EngineObjectValidationError, InvalidPayloadAttributesError,
    PayloadAttributes, PayloadOrAttributes,
};
use reth_primitives_traits::{
    Block, BlockHeader, NodePrimitives, RecoveredBlock, SealedBlock, SealedHeader,
};

// Custom consensus implementation that allows same-second timestamps for Malachite's sub-second block production.
#[derive(Debug, Clone)]
pub struct EmeraldConsensus {
    inner: reth_ethereum::consensus::EthBeaconConsensus<ChainSpec>,
}

impl EmeraldConsensus {
    pub fn new(chain_spec: Arc<ChainSpec>) -> Self {
        Self {
            inner: reth_ethereum::consensus::EthBeaconConsensus::new(chain_spec),
        }
    }

    // Validate timestamp allowing equal timestamps
    fn validate_timestamp<H: BlockHeader>(header: &H, parent: &H) -> Result<(), ConsensusError> {
        if header.timestamp() < parent.timestamp() {
            return Err(ConsensusError::TimestampIsInPast {
                parent_timestamp: parent.timestamp(),
                timestamp: header.timestamp(),
            });
        }
        Ok(())
    }
}

// Implement HeaderValidator with custom timestamp validation
impl<H> HeaderValidator<H> for EmeraldConsensus
where
    H: BlockHeader,
    ChainSpec: EthChainSpec<Header = H> + EthereumHardforks + Debug + Send + Sync,
{
    fn validate_header(&self, header: &SealedHeader<H>) -> Result<(), ConsensusError> {
        // Delegate to inner consensus
        self.inner.validate_header(header)
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader<H>,
        parent: &SealedHeader<H>,
    ) -> Result<(), ConsensusError> {
        validation::validate_against_parent_hash_number(header.header(), parent)?;

        // Use custom timestamp validation
        Self::validate_timestamp(header.header(), parent.header())?;

        validation::validate_against_parent_gas_limit(header, parent, &self.inner.chain_spec())?;

        validation::validate_against_parent_eip1559_base_fee(
            header.header(),
            parent.header(),
            &self.inner.chain_spec(),
        )?;

        if let Some(blob_params) = self
            .inner
            .chain_spec()
            .blob_params_at_timestamp(header.timestamp())
        {
            validation::validate_against_parent_4844(
                header.header(),
                parent.header(),
                blob_params,
            )?;
        }

        Ok(())
    }

    fn validate_header_range(
        &self,
        headers: &[SealedHeader<H>],
    ) -> Result<(), HeaderConsensusError<H>>
    where
        H: Clone,
    {
        self.inner.validate_header_range(headers)
    }
}

impl<B> Consensus<B> for EmeraldConsensus
where
    B: Block,
    ChainSpec: EthChainSpec<Header = B::Header> + EthereumHardforks + Debug + Send + Sync,
{
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        body: &B::Body,
        header: &SealedHeader<B::Header>,
    ) -> Result<(), Self::Error> {
        <reth_ethereum::consensus::EthBeaconConsensus<ChainSpec> as Consensus<B>>::validate_body_against_header(&self.inner, body, header)
    }

    fn validate_block_pre_execution(&self, block: &SealedBlock<B>) -> Result<(), Self::Error> {
        self.inner.validate_block_pre_execution(block)
    }
}

impl<N> FullConsensus<N> for EmeraldConsensus
where
    ChainSpec: Send + Sync + EthChainSpec<Header = N::BlockHeader> + EthereumHardforks + Debug,
    N: NodePrimitives,
{
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock<N::Block>,
        result: &BlockExecutionResult<N::Receipt>,
    ) -> Result<(), ConsensusError> {
        <reth_ethereum::consensus::EthBeaconConsensus<ChainSpec> as FullConsensus<N>>::validate_block_post_execution(&self.inner, block, result)
    }
}

// Builder for EmeraldConsensus
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EmeraldConsensusBuilder;

impl<Node> ConsensusBuilder<Node> for EmeraldConsensusBuilder
where
    Node: FullNodeTypes<Types: NodeTypes<ChainSpec = ChainSpec, Primitives = EthPrimitives>>,
{
    type Consensus = Arc<EmeraldConsensus>;

    async fn build_consensus(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Consensus> {
        Ok(Arc::new(EmeraldConsensus::new(ctx.chain_spec())))
    }
}

// Custom engine validator that allows same-second timestamps in Engine API
// by wrapping the standard EthereumEngineValidator and overriding the
// payload attributes timestamp validation.
#[derive(Debug, Clone)]
pub struct EmeraldEngineValidator {
    inner: EthereumEngineValidator<ChainSpec>,
}

impl EmeraldEngineValidator {
    pub fn new(chain_spec: Arc<ChainSpec>) -> Self {
        Self {
            inner: EthereumEngineValidator::new(chain_spec),
        }
    }
}

// Implement PayloadValidator with custom timestamp validation
impl<Types> PayloadValidator<Types> for EmeraldEngineValidator
where
    Types: PayloadTypes<
        ExecutionData = alloy_rpc_types_engine::ExecutionData,
        PayloadAttributes = alloy_rpc_types_engine::PayloadAttributes,
    >,
{
    type Block = reth_ethereum_primitives::Block;

    fn ensure_well_formed_payload(
        &self,
        payload: Types::ExecutionData,
    ) -> Result<RecoveredBlock<Self::Block>, reth_payload_primitives::NewPayloadError> {
        <EthereumEngineValidator<ChainSpec> as PayloadValidator<Types>>::ensure_well_formed_payload(
            &self.inner,
            payload,
        )
    }

    fn validate_payload_attributes_against_header(
        &self,
        attr: &Types::PayloadAttributes,
        header: &<Self::Block as Block>::Header,
    ) -> Result<(), InvalidPayloadAttributesError> {
        if attr.timestamp() < header.timestamp {
            return Err(InvalidPayloadAttributesError::InvalidTimestamp);
        }
        Ok(())
    }
}

impl<Types> EngineApiValidator<Types> for EmeraldEngineValidator
where
    Types: PayloadTypes<
        ExecutionData = alloy_rpc_types_engine::ExecutionData,
        PayloadAttributes = alloy_rpc_types_engine::PayloadAttributes,
    >,
{
    fn validate_version_specific_fields(
        &self,
        version: EngineApiMessageVersion,
        payload_or_attrs: PayloadOrAttributes<'_, Types::ExecutionData, Types::PayloadAttributes>,
    ) -> Result<(), EngineObjectValidationError> {
        <EthereumEngineValidator<ChainSpec> as EngineApiValidator<Types>>::validate_version_specific_fields(
            &self.inner,
            version,
            payload_or_attrs,
        )
    }

    fn ensure_well_formed_attributes(
        &self,
        version: EngineApiMessageVersion,
        attributes: &Types::PayloadAttributes,
    ) -> Result<(), EngineObjectValidationError> {
        <EthereumEngineValidator<ChainSpec> as EngineApiValidator<Types>>::ensure_well_formed_attributes(
            &self.inner,
            version,
            attributes,
        )
    }
}

// Builder for EmeraldEngineValidator
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EmeraldEngineValidatorBuilder;

impl<Node> PayloadValidatorBuilder<Node> for EmeraldEngineValidatorBuilder
where
    Node: FullNodeComponents<Types: NodeTypes<ChainSpec = ChainSpec>>,
    <Node::Types as NodeTypes>::Payload: PayloadTypes<
        ExecutionData = alloy_rpc_types_engine::ExecutionData,
        PayloadAttributes = alloy_rpc_types_engine::PayloadAttributes,
    >,
{
    type Validator = EmeraldEngineValidator;

    async fn build(self, ctx: &AddOnsContext<'_, Node>) -> eyre::Result<Self::Validator> {
        Ok(EmeraldEngineValidator::new(ctx.config.chain.clone()))
    }
}
