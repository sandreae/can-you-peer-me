use std::hash::Hash as StdHash;

use p2panda_core::PrivateKey;
use p2panda_discovery::mdns::LocalDiscovery;
use p2panda_net::{FromNetwork, Network, NetworkBuilder, ToNetwork, TopicId};
use p2panda_sync::TopicQuery;
use serde::{ser::SerializeStruct, Deserialize, Serialize};
use tauri::{ipc::Channel, AppHandle, Builder, Manager, State};
use thiserror::Error;
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
    channel: Option<Channel<ChannelEvent>>,
    network: Network<AppTopic>,
    topic_tx: mpsc::Sender<ToNetwork>,
    app_tx: mpsc::Sender<(u64, u16)>,
}

#[tauri::command]
async fn init(
    state: State<'_, Mutex<AppContext>>,
    channel: Channel<ChannelEvent>,
) -> Result<(), InitError> {
    let mut state = state.lock().await;
    state.channel = Some(channel);

    Ok(())
}

#[tauri::command]
async fn broadcast(state: State<'_, Mutex<AppContext>>, timestamp: u64, index: u16) -> Result<(), InitError> {
    let mut state = state.lock().await;
    let message = (timestamp, index);
    state
        .app_tx
        .send(message)
        .await
        .expect("send on app_tx channel");
    state
        .topic_tx
        .send(ToNetwork::Message {
            bytes: serde_json::to_vec(&message).expect("encode message as vec"),
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

                let network = NetworkBuilder::new(network_id)
                    .discovery(mdns)
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

                let (app_tx, app_rx) = mpsc::channel(32);

                app_handle.manage(Mutex::new(AppContext {
                    channel: None,
                    network,
                    topic_tx,
                    app_tx,
                }));

                if let Err(err) =
                    forward_to_app_layer(app_handle, topic_rx, app_rx, system_events_rx).await
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
    app_handle: AppHandle,
    mut topic_rx: mpsc::Receiver<FromNetwork>,
    mut app_rx: mpsc::Receiver<(u64, u16)>,
    mut system_events_rx: broadcast::Receiver<p2panda_net::SystemEvent<AppTopic>>,
) -> anyhow::Result<()> {
    let rt = tokio::runtime::Handle::current();

    rt.spawn(async move {
        loop {
            tokio::select! {
                Ok(event) = system_events_rx.recv() => {
                    let state = app_handle.state::<Mutex<AppContext>>();
                    let lock = state.lock().await;        
                    if let Some(channel) = &lock.channel {
                        channel.send(ChannelEvent::SystemEvent(SystemEvent(event))).expect("send on app channel");
                    }
                },
                Some(event) = topic_rx.recv() => {
                    let state = app_handle.state::<Mutex<AppContext>>();
                    let lock = state.lock().await;        
                    let (timestamp, index): (u64, u16) = match event {
                        FromNetwork::GossipMessage { ref bytes, delivered_from } => {
                            serde_json::from_slice(bytes).expect("decode message bytes")
                        },
                        FromNetwork::SyncMessage { header, payload, delivered_from } => todo!(),
                    };

                    if let Some(channel) = &lock.channel {
                        channel.send(ChannelEvent::SamplePlayed{timestamp, index}).expect("send on app channel");
                    }
                },
                Some((timestamp, index)) = app_rx.recv() => {
                    let state = app_handle.state::<Mutex<AppContext>>();
                    let lock = state.lock().await;        
                    if let Some(channel) = &lock.channel {
                        println!("forward message to app: {}", index);
                        channel.send(ChannelEvent::SamplePlayed{timestamp, index}).expect("send on app channel");
                    }
                },
            }
        }
    });

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum StreamEvent {
    Message,
}

#[derive(Debug, Clone)]
enum ChannelEvent {
    SamplePlayed{
        timestamp: u64,
        index: u16
    },
    SystemEvent(SystemEvent),
}

impl Serialize for ChannelEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            ChannelEvent::SamplePlayed{ ref timestamp, ref index} => {
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

struct TopicMap {}

#[derive(Clone, Debug, Error, Serialize, Deserialize)]
enum InitError {
    #[error("oneshot channel receiver closed")]
    OneshotChannelError,

    #[error("stream channel already set")]
    SetStreamChannelError,
}
