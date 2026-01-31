//! Block type for simplex consensus with EVM execution.

use alloy_primitives::B256;
use alloy_rpc_types_engine::ExecutionPayloadV3;
use alloy_rpc_types_eth::Block as RpcBlock;
use bytes::{Buf, BufMut};
use commonware_codec::varint::UInt;
use commonware_codec::{EncodeSize, Error, Read, ReadExt, Write};
use commonware_consensus::types::Height;
use commonware_consensus::Heightable;
use commonware_cryptography::sha256::Digest;
use commonware_cryptography::{Committable, Digestible, Hasher, Sha256};
use ssz::{Decode, Encode};

/// Execution block hash from the EVM execution layer.
pub type ExecutionHash = B256;

/// Minimal execution block data used by simplex consensus.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionBlock {
    pub block_hash: B256,
    pub block_number: u64,
    pub parent_hash: B256,
    pub timestamp: u64,
    pub prev_randao: B256,
}

impl ExecutionBlock {
    pub fn from_rpc_block(block: RpcBlock) -> Self {
        let header = block.header;
        Self {
            block_hash: header.hash,
            block_number: header.inner.number,
            parent_hash: header.inner.parent_hash,
            timestamp: header.inner.timestamp,
            prev_randao: header.inner.mix_hash,
        }
    }
}

/// Block for simplex consensus with EVM execution.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The parent block's digest.
    pub parent: Digest,

    /// The height of the block in the blockchain.
    pub height: Height,

    /// The timestamp of the block (in milliseconds since the Unix epoch).
    pub timestamp: u64,

    /// EVM execution fields.
    evm: EvmFields,

    /// Pre-computed digest of the block.
    digest: Digest,
}

/// EVM-specific fields in the block.
#[derive(Clone, Debug, PartialEq)]
pub struct EvmFields {
    /// Execution block data from the EVM execution layer.
    pub execution_block: ExecutionBlock,

    /// The full execution payload (optional).
    /// Included in proposals for verification, can be dropped after finalization.
    pub execution_payload: Option<ExecutionPayloadV3>,
}

impl Block {
    fn compute_digest(
        parent: &Digest,
        height: Height,
        timestamp: u64,
        execution_hash: &ExecutionHash,
    ) -> Digest {
        let mut hasher = Sha256::new();
        hasher.update(parent);
        hasher.update(&height.get().to_be_bytes());
        hasher.update(&timestamp.to_be_bytes());
        hasher.update(execution_hash.as_slice());
        hasher.finalize()
    }

    /// Create a new block without execution payload.
    pub fn new(
        parent: Digest,
        height: Height,
        timestamp: u64,
        execution_block: ExecutionBlock,
    ) -> Self {
        let digest = Self::compute_digest(&parent, height, timestamp, &execution_block.block_hash);
        Self {
            parent,
            height,
            timestamp,
            evm: EvmFields {
                execution_block,
                execution_payload: None,
            },
            digest,
        }
    }

    /// Create a new block with execution payload.
    pub fn new_with_payload(parent: Digest, execution_payload: ExecutionPayloadV3) -> Self {
        let payload_inner = &execution_payload.payload_inner.payload_inner;
        let height = Height::new(payload_inner.block_number);
        let timestamp = payload_inner.timestamp.saturating_mul(1_000);
        let execution_block = ExecutionBlock {
            block_hash: payload_inner.block_hash,
            block_number: payload_inner.block_number,
            parent_hash: payload_inner.parent_hash,
            timestamp: payload_inner.timestamp,
            prev_randao: payload_inner.prev_randao,
        };
        let digest = Self::compute_digest(&parent, height, timestamp, &execution_block.block_hash);
        Self {
            parent,
            height,
            timestamp,
            evm: EvmFields {
                execution_block,
                execution_payload: Some(execution_payload),
            },
            digest,
        }
    }

    /// Get the execution payload, if present.
    pub fn payload(&self) -> Option<&ExecutionPayloadV3> {
        self.evm.execution_payload.as_ref()
    }

    /// Take the execution payload out of the block.
    pub fn take_payload(&mut self) -> Option<ExecutionPayloadV3> {
        self.evm.execution_payload.take()
    }

    /// Get the execution hash.
    pub fn execution_hash(&self) -> ExecutionHash {
        self.evm.execution_block.block_hash
    }

    /// Get the parent execution hash.
    pub fn parent_execution_hash(&self) -> ExecutionHash {
        self.evm.execution_block.parent_hash
    }

    /// Get the execution block.
    pub fn execution_block(&self) -> &ExecutionBlock {
        &self.evm.execution_block
    }
}

impl Write for Block {
    fn write(&self, writer: &mut impl BufMut) {
        self.parent.write(writer);
        self.height.write(writer);
        UInt(self.timestamp).write(writer);
        writer.put_slice(self.evm.execution_block.block_hash.as_slice());
        writer.put_slice(self.evm.execution_block.parent_hash.as_slice());
        writer.put_slice(self.evm.execution_block.prev_randao.as_slice());
        UInt(self.evm.execution_block.block_number).write(writer);
        UInt(self.evm.execution_block.timestamp).write(writer);

        match &self.evm.execution_payload {
            Some(payload) => {
                writer.put_u8(1);
                let ssz_bytes = payload.as_ssz_bytes();
                UInt(ssz_bytes.len() as u64).write(writer);
                writer.put_slice(&ssz_bytes);
            }
            None => {
                writer.put_u8(0);
            }
        }
    }
}

impl Read for Block {
    type Cfg = ();

    fn read_cfg(reader: &mut impl Buf, _: &Self::Cfg) -> Result<Self, Error> {
        let parent = Digest::read(reader)?;
        let height = Height::read(reader)?;
        let timestamp = UInt::read(reader)?.into();

        if reader.remaining() < 96 {
            return Err(Error::EndOfBuffer);
        }
        let mut exec_hash = [0u8; 32];
        reader.copy_to_slice(&mut exec_hash);
        let execution_hash = B256::from(exec_hash);
        let mut parent_exec_hash = [0u8; 32];
        reader.copy_to_slice(&mut parent_exec_hash);
        let parent_execution_hash = B256::from(parent_exec_hash);
        let mut prev_randao_bytes = [0u8; 32];
        reader.copy_to_slice(&mut prev_randao_bytes);
        let prev_randao = B256::from(prev_randao_bytes);
        let block_number = UInt::read(reader)?.into();
        let exec_timestamp = UInt::read(reader)?.into();

        let execution_payload = if reader.remaining() >= 1 {
            let marker = reader.get_u8();
            if marker == 1 {
                let payload_len: u64 = UInt::read(reader)?.into();
                let payload_len = payload_len as usize;
                if reader.remaining() < payload_len {
                    return Err(Error::EndOfBuffer);
                }
                let mut payload_bytes = vec![0u8; payload_len];
                reader.copy_to_slice(&mut payload_bytes);
                let payload = ExecutionPayloadV3::from_ssz_bytes(&payload_bytes)
                    .map_err(|_| Error::Invalid("Block", "failed to decode SSZ payload"))?;
                Some(payload)
            } else if marker == 0 {
                None
            } else {
                return Err(Error::Invalid("Block", "invalid payload marker"));
            }
        } else {
            None
        };

        let digest = Self::compute_digest(&parent, height, timestamp, &execution_hash);
        Ok(Self {
            parent,
            height,
            timestamp,
            evm: EvmFields {
                execution_block: ExecutionBlock {
                    block_hash: execution_hash,
                    block_number,
                    parent_hash: parent_execution_hash,
                    timestamp: exec_timestamp,
                    prev_randao,
                },
                execution_payload,
            },
            digest,
        })
    }
}

impl EncodeSize for Block {
    fn encode_size(&self) -> usize {
        let mut size = self.parent.encode_size()
            + self.height.encode_size()
            + UInt(self.timestamp).encode_size()
            + 96
            + UInt(self.evm.execution_block.block_number).encode_size()
            + UInt(self.evm.execution_block.timestamp).encode_size();

        size += 1;
        if let Some(payload) = &self.evm.execution_payload {
            let ssz_len = payload.ssz_bytes_len();
            size += UInt(ssz_len as u64).encode_size();
            size += ssz_len;
        }

        size
    }
}

impl Digestible for Block {
    type Digest = Digest;

    fn digest(&self) -> Digest {
        self.digest
    }
}

impl Committable for Block {
    type Commitment = Digest;

    fn commitment(&self) -> Digest {
        self.digest
    }
}

impl commonware_consensus::Block for Block {
    fn parent(&self) -> Digest {
        self.parent
    }
}

impl Heightable for Block {
    fn height(&self) -> Height {
        self.height
    }
}
