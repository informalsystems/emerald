//! Block type for simplex consensus with EVM execution.

use core::mem;

use alloy_consensus::Header as ConsensusHeader;
use alloy_primitives::B256;
use alloy_rlp::{Decodable, Encodable};
use alloy_rpc_types_engine::ExecutionPayloadV3;
use alloy_rpc_types_eth::Header;
use bytes::{Buf, BufMut};
use commonware_codec::varint::UInt;
use commonware_codec::{EncodeSize, Error, Read, ReadExt, Write};
use commonware_consensus::types::Height;
use commonware_consensus::Heightable;
use commonware_cryptography::sha256::Digest;
use commonware_cryptography::{Committable, Digestible, Hasher, Sha256};
use ssz::{Decode, Encode};

/// Block for simplex consensus with EVM execution.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The parent block's digest.
    pub parent: Digest,

    /// The height of the block in the blockchain.
    pub height: Height,

    /// The timestamp of the block (in seconds since the Unix epoch).
    pub timestamp: u64,

    /// Execution data (header/hash with optional payload).
    execution_data: ExecutionData,

    /// Pre-computed digest of the block.
    digest: Digest,
}

/// Execution data retained by the consensus block.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionData {
    Genesis { header: ConsensusHeader, hash: B256 },
    Payload { payload: ExecutionPayloadV3 },
}

impl ExecutionData {
    pub fn from_rpc_header(header: Header) -> Self {
        Self::Genesis {
            header: header.inner,
            hash: header.hash,
        }
    }
}

impl Block {
    fn compute_digest(
        parent: &Digest,
        height: Height,
        timestamp: u64,
        execution_hash: &B256,
    ) -> Digest {
        let mut hasher = Sha256::new();
        hasher.update(parent);
        hasher.update(&height.get().to_be_bytes());
        hasher.update(&timestamp.to_be_bytes());
        hasher.update(execution_hash.as_slice());
        hasher.finalize()
    }

    /// Create a new block without execution payload.
    pub fn new(parent: Digest, execution_data: ExecutionData) -> Self {
        let (height, timestamp, execution_hash) = match &execution_data {
            ExecutionData::Genesis { header, hash } => {
                let height = Height::new(header.number);
                let timestamp = header.timestamp;
                (height, timestamp, *hash)
            }
            ExecutionData::Payload { payload } => {
                let payload_inner = &payload.payload_inner.payload_inner;
                let height = Height::new(payload_inner.block_number);
                let timestamp = payload_inner.timestamp;
                (height, timestamp, payload_inner.block_hash)
            }
        };
        let digest = Self::compute_digest(&parent, height, timestamp, &execution_hash);
        Self {
            parent,
            height,
            timestamp,
            execution_data,
            digest,
        }
    }

    /// Create a new block with execution payload.
    pub fn new_with_payload(parent: Digest, execution_payload: ExecutionPayloadV3) -> Self {
        let execution_data = ExecutionData::Payload {
            payload: execution_payload,
        };
        Self::new(parent, execution_data)
    }

    /// Get the execution payload, if present.
    pub fn payload(&self) -> Option<&ExecutionPayloadV3> {
        match &self.execution_data {
            ExecutionData::Payload { payload } => Some(payload),
            ExecutionData::Genesis { .. } => None,
        }
    }

    /// Take the execution payload out of the block.
    pub fn take_payload(&mut self) -> Option<ExecutionPayloadV3> {
        match mem::replace(
            &mut self.execution_data,
            ExecutionData::Genesis {
                header: ConsensusHeader::default(),
                hash: B256::ZERO,
            },
        ) {
            ExecutionData::Payload { payload } => Some(payload),
            ExecutionData::Genesis { header, hash } => {
                self.execution_data = ExecutionData::Genesis { header, hash };
                None
            }
        }
    }

    /// Get the execution hash.
    pub fn execution_hash(&self) -> B256 {
        match &self.execution_data {
            ExecutionData::Genesis { hash, .. } => *hash,
            ExecutionData::Payload { payload } => payload.payload_inner.payload_inner.block_hash,
        }
    }

    /// Get the parent execution hash.
    pub fn parent_execution_hash(&self) -> B256 {
        match &self.execution_data {
            ExecutionData::Genesis { header, .. } => header.parent_hash,
            ExecutionData::Payload { payload } => payload.payload_inner.payload_inner.parent_hash,
        }
    }

    pub fn prev_randao(&self) -> B256 {
        match &self.execution_data {
            ExecutionData::Genesis { header, .. } => header.mix_hash,
            ExecutionData::Payload { payload } => payload.payload_inner.payload_inner.prev_randao,
        }
    }
}

impl Write for Block {
    fn write(&self, writer: &mut impl BufMut) {
        self.parent.write(writer);
        self.height.write(writer);
        UInt(self.timestamp).write(writer);
        match &self.execution_data {
            ExecutionData::Genesis { header, hash } => {
                writer.put_u8(0);
                writer.put_slice(hash.as_slice());
                let header_len = header.length();
                UInt(header_len as u64).write(writer);
                header.encode(writer);
            }
            ExecutionData::Payload { payload } => {
                writer.put_u8(1);
                let ssz_bytes = payload.as_ssz_bytes();
                UInt(ssz_bytes.len() as u64).write(writer);
                writer.put_slice(&ssz_bytes);
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

        if reader.remaining() < 1 {
            return Err(Error::EndOfBuffer);
        }

        let tag = reader.get_u8();
        let execution_data = match tag {
            0 => {
                if reader.remaining() < 32 {
                    return Err(Error::EndOfBuffer);
                }
                let mut exec_hash = [0u8; 32];
                reader.copy_to_slice(&mut exec_hash);
                let execution_hash = B256::from(exec_hash);

                let header_len: u64 = UInt::read(reader)?.into();
                let header_len = header_len as usize;
                if reader.remaining() < header_len {
                    return Err(Error::EndOfBuffer);
                }
                let mut header_bytes = vec![0u8; header_len];
                reader.copy_to_slice(&mut header_bytes);
                let mut header_slice = header_bytes.as_slice();
                let header = ConsensusHeader::decode(&mut header_slice)
                    .map_err(|_| Error::Invalid("Block", "failed to decode header"))?;
                ExecutionData::Genesis {
                    header,
                    hash: execution_hash,
                }
            }
            1 => {
                let payload_len: u64 = UInt::read(reader)?.into();
                let payload_len = payload_len as usize;
                if reader.remaining() < payload_len {
                    return Err(Error::EndOfBuffer);
                }
                let mut payload_bytes = vec![0u8; payload_len];
                reader.copy_to_slice(&mut payload_bytes);
                let payload = ExecutionPayloadV3::from_ssz_bytes(&payload_bytes)
                    .map_err(|_| Error::Invalid("Block", "failed to decode SSZ payload"))?;
                ExecutionData::Payload { payload }
            }
            _ => return Err(Error::Invalid("Block", "invalid execution data tag")),
        };

        let execution_hash = match &execution_data {
            ExecutionData::Genesis { hash, .. } => *hash,
            ExecutionData::Payload { payload } => payload.payload_inner.payload_inner.block_hash,
        };
        let digest = Self::compute_digest(&parent, height, timestamp, &execution_hash);
        Ok(Self {
            parent,
            height,
            timestamp,
            execution_data,
            digest,
        })
    }
}

impl EncodeSize for Block {
    fn encode_size(&self) -> usize {
        let mut size = self.parent.encode_size()
            + self.height.encode_size()
            + UInt(self.timestamp).encode_size()
            + 1;

        match &self.execution_data {
            ExecutionData::Genesis { header, .. } => {
                let header_len = header.length();
                size += 32 + UInt(header_len as u64).encode_size() + header_len;
            }
            ExecutionData::Payload { payload } => {
                let ssz_len = payload.ssz_bytes_len();
                size += UInt(ssz_len as u64).encode_size() + ssz_len;
            }
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
