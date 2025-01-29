use serde::ser::SerializeStruct;
use serde::Serialize;

use crate::AppTopic;

#[derive(Debug, Clone)]
pub enum ChannelEvent {
    SamplePlayed { timestamp: u64, index: u16 },
    SystemEvent(SystemEvent),
}

#[derive(Debug, Clone)]
pub struct SystemEvent(pub(crate) p2panda_net::SystemEvent<AppTopic>);

impl Serialize for ChannelEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            ChannelEvent::SamplePlayed {
                ref timestamp,
                ref index,
            } => {
                let mut state = serializer.serialize_struct("ChannelEvent", 2)?;
                state.serialize_field("type", "SamplePlayed")?;
                state.serialize_field("timestamp", timestamp)?;
                state.serialize_field("index", index)?;
                state.end()
            }
            ChannelEvent::SystemEvent(ref event) => {
                let mut state = serializer.serialize_struct("ChannelEvent", 2)?;
                state.serialize_field("type", "SystemEvent")?;
                state.serialize_field("data", event)?;
                state.end()
            }
        }
    }
}

impl Serialize for SystemEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match &self.0 {
            p2panda_net::SystemEvent::GossipJoined { topic_id, peers } => {
                let mut state = serializer.serialize_struct("SystemEvent", 3)?;
                state.serialize_field("type", "GossipJoined")?;
                state.serialize_field("topic_id", topic_id)?;
                state.serialize_field("peers", peers)?;
                state.end()
            }
            p2panda_net::SystemEvent::GossipLeft { topic_id } => {
                let mut state = serializer.serialize_struct("SystemEvent", 2)?;
                state.serialize_field("type", "GossipLeft")?;
                state.serialize_field("topic_id", topic_id)?;
                state.end()
            }
            p2panda_net::SystemEvent::GossipNeighborUp { topic_id, peer } => {
                let mut state = serializer.serialize_struct("SystemEvent", 3)?;
                state.serialize_field("type", "GossipNeighborUp")?;
                state.serialize_field("topic_id", topic_id)?;
                state.serialize_field("peer", peer)?;
                state.end()
            }
            p2panda_net::SystemEvent::GossipNeighborDown { topic_id, peer } => {
                let mut state = serializer.serialize_struct("SystemEvent", 3)?;
                state.serialize_field("type", "GossipNeighborDown")?;
                state.serialize_field("topic_id", topic_id)?;
                state.serialize_field("peer", peer)?;
                state.end()
            }
            p2panda_net::SystemEvent::PeerDiscovered { peer } => {
                let mut state = serializer.serialize_struct("SystemEvent", 2)?;
                state.serialize_field("type", "PeerDiscovered")?;
                state.serialize_field("peer", peer)?;
                state.end()
            }
            p2panda_net::SystemEvent::SyncStarted { topic, peer } => {
                let mut state = serializer.serialize_struct("SystemEvent", 3)?;
                state.serialize_field("type", "SyncStarted")?;
                state.serialize_field("topic", topic)?;
                state.serialize_field("peer", peer)?;
                state.end()
            }
            p2panda_net::SystemEvent::SyncDone { topic, peer } => {
                let mut state = serializer.serialize_struct("SystemEvent", 3)?;
                state.serialize_field("type", "SyncDone")?;
                state.serialize_field("topic", topic)?;
                state.serialize_field("peer", peer)?;
                state.end()
            }
            p2panda_net::SystemEvent::SyncFailed { topic, peer } => {
                let mut state = serializer.serialize_struct("SystemEvent", 3)?;
                state.serialize_field("type", "SyncFailed")?;
                state.serialize_field("topic", topic)?;
                state.serialize_field("peer", peer)?;
                state.end()
            }
        }
    }
}
