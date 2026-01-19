use core::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashSet};

use malachitebft_app_channel::app::streaming::{Sequence, StreamId, StreamMessage};
use malachitebft_app_channel::app::types::core::Round;
use malachitebft_app_channel::app::types::PeerId;
use malachitebft_eth_types::{Address, Height, ProposalFin, ProposalInit, ProposalPart};

struct MinSeq<T>(StreamMessage<T>);

impl<T> PartialEq for MinSeq<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.sequence == other.0.sequence
    }
}

impl<T> Eq for MinSeq<T> {}

impl<T> Ord for MinSeq<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.sequence.cmp(&self.0.sequence)
    }
}

impl<T> PartialOrd for MinSeq<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct MinHeap<T>(BinaryHeap<MinSeq<T>>);

impl<T> Default for MinHeap<T> {
    fn default() -> Self {
        Self(BinaryHeap::new())
    }
}

impl<T> MinHeap<T> {
    fn push(&mut self, msg: StreamMessage<T>) {
        self.0.push(MinSeq(msg));
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn drain(&mut self) -> Vec<T> {
        let mut vec = Vec::with_capacity(self.0.len());
        while let Some(MinSeq(msg)) = self.0.pop() {
            if let Some(data) = msg.content.into_data() {
                vec.push(data);
            }
        }
        vec
    }
}

#[derive(Default)]
struct StreamState {
    buffer: MinHeap<ProposalPart>,
    init_info: Option<ProposalInit>,
    seen_sequences: HashSet<Sequence>,
    total_messages: usize,
    fin_received: bool,
}

enum StreamProgress {
    Incomplete(StreamState),
    Complete(ProposalParts),
}

impl StreamState {
    fn is_done(&self) -> bool {
        self.init_info.is_some() && self.fin_received && self.buffer.len() == self.total_messages
    }

    fn insert(mut self, msg: StreamMessage<ProposalPart>) -> StreamProgress {
        if self.seen_sequences.insert(msg.sequence) {
            if msg.is_first() {
                self.init_info = msg.content.as_data().and_then(|p| p.as_init()).cloned();
            }

            if msg.is_fin() {
                self.fin_received = true;
                self.total_messages = msg.sequence as usize + 1;
            }

            self.buffer.push(msg);

            if self.is_done() {
                let init_info = self.init_info.take().expect("init_info must exist if done");

                let parts = ProposalParts {
                    height: init_info.height,
                    round: init_info.round,
                    proposer: init_info.proposer,
                    parts: self.buffer.drain(),
                };

                return StreamProgress::Complete(parts);
            }
        }

        StreamProgress::Incomplete(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalParts {
    pub height: Height,
    pub round: Round,
    pub proposer: Address,
    pub parts: Vec<ProposalPart>,
}

impl ProposalParts {
    pub fn init(&self) -> Option<&ProposalInit> {
        self.parts.iter().find_map(|p| p.as_init())
    }

    pub fn fin(&self) -> Option<&ProposalFin> {
        self.parts.iter().find_map(|p| p.as_fin())
    }
}

#[derive(Default)]
pub struct PartStreamsMap {
    streams: BTreeMap<(PeerId, StreamId), StreamState>,
}

impl PartStreamsMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        peer_id: PeerId,
        msg: StreamMessage<ProposalPart>,
    ) -> Option<ProposalParts> {
        let stream_id = msg.stream_id.clone();
        let stream_key = (peer_id, stream_id);
        let state_ref = self.streams.entry(stream_key.clone()).or_default();

        // Temporarily take ownership over the stream state since it's consumed
        // by `insert`. Return ownership if the stream isn't completed yet.
        let state = core::mem::take(state_ref);

        match state.insert(msg) {
            StreamProgress::Incomplete(state) => {
                *state_ref = state;
                None
            }
            StreamProgress::Complete(parts) => {
                self.streams.remove(&stream_key);
                Some(parts)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use malachitebft_app_channel::app::streaming::StreamContent;
    use malachitebft_eth_types::secp256k1::Signature;
    use malachitebft_eth_types::ProposalData;

    use super::*;

    #[test]
    fn test_insert_prune_completed_streams() {
        let peer_id = PeerId::from_multihash(Default::default()).unwrap();
        let stream_id = StreamId::new(Bytes::new());
        let address = Address::new([0; 20]);
        let signature = Signature::from_slice(&[
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0,
        ])
        .unwrap();

        let init = ProposalPart::Init(ProposalInit::new(
            Height::new(1),
            Round::Some(0),
            Round::Nil,
            address,
        ));
        let data = ProposalPart::Data(ProposalData::new(Bytes::new()));
        let fin = ProposalPart::Fin(ProposalFin::new(signature));

        let part0 = StreamMessage::new(stream_id.clone(), 0, StreamContent::Data(init));
        let part1 = StreamMessage::new(stream_id.clone(), 1, StreamContent::Data(data));
        let part2 = StreamMessage::new(stream_id.clone(), 2, StreamContent::Data(fin));
        let part3 = StreamMessage::new(stream_id, 3, StreamContent::Fin);

        let mut streams_map = PartStreamsMap::new();
        assert!(streams_map.insert(peer_id, part0).is_none()); // incomplete
        assert!(
            !streams_map.streams.is_empty(),
            "streams map must track active stream"
        );
        assert!(streams_map.insert(peer_id, part1.clone()).is_none()); // incomplete
        assert!(streams_map.insert(peer_id, part1).is_none()); // repeated seq; no-op
        assert!(streams_map.insert(peer_id, part2).is_none()); // incomplete
        assert!(streams_map.insert(peer_id, part3).is_some()); // complete
        assert!(
            streams_map.streams.is_empty(),
            "streams map must drop complete streams"
        );
    }
}
