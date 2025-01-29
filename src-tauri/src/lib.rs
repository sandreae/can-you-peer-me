mod sync;

use std::hash::Hash as StdHash;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::PrivateKey;
use p2panda_discovery::mdns::LocalDiscovery;
use p2panda_net::{
    FromNetwork, Network, NetworkBuilder, ResyncConfiguration, SyncConfiguration, ToNetwork,
    TopicId,
};
use p2panda_sync::TopicQuery;
use serde::{ser::SerializeStruct, Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{Builder, Error, Manager, State};
use tokio::sync::{broadcast, mpsc, Mutex};

static network_id: [u8; 32] = [0; 32];
static app_topic: AppTopic = AppTopic([1; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
struct AppTopic([u8; 32]);

impl TopicId for AppTopic {
    fn id(&self) -> [u8; 32] {
        self.0
    }
}

impl TopicQuery for AppTopic {}

struct AppContext {
    channel_init_tx: mpsc::Sender<Channel<ChannelEvent>>,
    network: Network<AppTopic>,
    topic_tx: mpsc::Sender<ToNetwork>,
    app_tx: mpsc::Sender<(u64, u16)>,
}

#[tauri::command]
async fn init(
    state: State<'_, Mutex<AppContext>>,
    channel: Channel<ChannelEvent>,
) -> Result<(), Error> {
    let state = state.lock().await;
    state
        .channel_init_tx
        .send(channel)
        .await
        .expect("send on init channel");

    Ok(())
}

#[tauri::command]
async fn broadcast(
    state: State<'_, Mutex<AppContext>>,
    timestamp: u64,
    index: u16,
) -> Result<(), Error> {
    let state = state.lock().await;
    let message = (timestamp, index);
    state
        .app_tx
        .send(message)
        .await
        .expect("send on app_tx channel");
    state
        .topic_tx
        .send(ToNetwork::Message {
            bytes: encode_cbor(&message).expect("encode message"),
        })
        .await
        .expect("send on topic_tx channel");

    println!("broadcast message: {}", index);

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                let private_key = PrivateKey::new();

                let mdns = LocalDiscovery::new();
                let sync_protocol = sync::DummyProtocol {};
                let resync_config = ResyncConfiguration::new().interval(10);
                let sync_config = SyncConfiguration::new(sync_protocol).resync(resync_config);

                let network = NetworkBuilder::new(network_id)
                    .discovery(mdns)
                    .sync(sync_config)
                    .private_key(private_key.clone())
                    .build()
                    .await
                    .expect("build network");

                let system_events_rx = network
                    .events()
                    .await
                    .expect("subscribe to network system status event stream");

                let (topic_tx, topic_rx, _topic_ready) = network
                    .subscribe(app_topic)
                    .await
                    .expect("subscribe to topic");

                let (channel_init_tx, channel_init_rx) = mpsc::channel(32);
                let (app_tx, app_rx) = mpsc::channel(32);

                app_handle.manage(Mutex::new(AppContext {
                    channel_init_tx,
                    network,
                    topic_tx,
                    app_tx,
                }));

                if let Err(err) =
                    forward_to_app_layer(channel_init_rx, topic_rx, app_rx, system_events_rx).await
                {
                    panic!("failed to start node receiver task: {err}")
                };
            });

            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![init, broadcast])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Task for receiving data from network and forwarding them up to the app layer.
async fn forward_to_app_layer(
    mut channel_init_rx: mpsc::Receiver<Channel<ChannelEvent>>,
    mut topic_rx: mpsc::Receiver<FromNetwork>,
    mut app_rx: mpsc::Receiver<(u64, u16)>,
    mut system_events_rx: broadcast::Receiver<p2panda_net::SystemEvent<AppTopic>>,
) -> anyhow::Result<()> {
    let rt = tokio::runtime::Handle::current();

    rt.spawn(async move {
        let mut channel = channel_init_rx
        .recv()
        .await
        .expect("channel arrives on channel init receiver");

        loop {
            tokio::select! {
                Ok(event) = system_events_rx.recv() => {
                        channel.send(ChannelEvent::SystemEvent(SystemEvent(event))).expect("send on app channel");
                },
                Some(event) = topic_rx.recv() => {
                    let (timestamp, index): (u64, u16) = match event {
                        FromNetwork::GossipMessage { ref bytes, delivered_from } => {
                            decode_cbor(&bytes[..]).expect("decode message bytes")
                        },
                        FromNetwork::SyncMessage { header, payload, delivered_from } => todo!(),
                    };

                        channel.send(ChannelEvent::SamplePlayed{timestamp, index}).expect("send on app channel");
                },
                Some((timestamp, index)) = app_rx.recv() => {
                    println!("forward message to app: {}", index);
                    channel.send(ChannelEvent::SamplePlayed{timestamp, index}).expect("send on app channel");
            },
            Some(new_channel) = channel_init_rx.recv() => {
                channel = new_channel
        },
}
        }
    });

    Ok(())
}

#[derive(Debug, Clone)]
enum ChannelEvent {
    SamplePlayed { timestamp: u64, index: u16 },
    SystemEvent(SystemEvent),
}

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

#[derive(Debug, Clone)]
struct SystemEvent(p2panda_net::SystemEvent<AppTopic>);

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
