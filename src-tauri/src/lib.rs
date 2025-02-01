mod messages;
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
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{App, Builder, Error, Manager, State};
use tokio::sync::{mpsc, Mutex};

use messages::{ApplicationMessage, ChannelEvent, SystemEvent};

static NETWORK_ID: [u8; 32] = [0; 32];
static APP_TOPIC: AppTopic = AppTopic([1; 32]);

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
    #[allow(dead_code)]
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
async fn publish(
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

    println!("message published: {:?}", message);

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    Builder::default()
        .setup(|app| {
            spawn_node(app)?;
            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![init, publish])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn spawn_node(app: &mut App) -> Result<(), Error> {
    let app_handle = app.handle().clone();

    tauri::async_runtime::spawn(async move {
        let private_key = PrivateKey::new();
        let network = build_network(private_key.clone())
            .await
            .expect("build network");

        let mut system_events_rx = network
            .events()
            .await
            .expect("subscribe to network system status event stream");

        let (topic_tx, mut topic_rx, _topic_ready) = network
            .subscribe(APP_TOPIC)
            .await
            .expect("subscribe to topic");

        let (channel_init_tx, mut channel_init_rx) = mpsc::channel(32);
        let (app_tx, mut app_rx) = mpsc::channel(32);

        app_handle.manage(Mutex::new(AppContext {
            channel_init_tx,
            network,
            topic_tx,
            app_tx,
        }));

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
                        FromNetwork::GossipMessage { ref bytes, .. } => {
                            decode_cbor(&bytes[..]).expect("decode message bytes")
                        },
                        // We don't expect to receive any messages via sync.
                        FromNetwork::SyncMessage { .. } => todo!(),
                    };

                        channel.send(ChannelEvent::ApplicationMessage(ApplicationMessage {
                            timestamp,
                            sample_index: index,
                            public_key: private_key.public_key()
                        })).expect("send on app channel");
                },
                Some((timestamp, index)) = app_rx.recv() => {
                    channel.send(ChannelEvent::ApplicationMessage(ApplicationMessage {
                        timestamp,
                        sample_index: index,
                        public_key: private_key.public_key()
                    })).expect("send on app channel");
                },
                Some(new_channel) = channel_init_rx.recv() => {
                    channel = new_channel
                },
            }
        }
    });

    Ok(())
}

async fn build_network(private_key: PrivateKey) -> anyhow::Result<Network<AppTopic>> {
    let mdns = LocalDiscovery::new();
    let sync_protocol = sync::DummyProtocol {};
    let resync_config = ResyncConfiguration::new().interval(10);
    let sync_config = SyncConfiguration::new(sync_protocol).resync(resync_config);

    let network = NetworkBuilder::new(NETWORK_ID)
        .discovery(mdns)
        .sync(sync_config)
        .private_key(private_key.clone())
        .build()
        .await?;

    Ok(network)
}
