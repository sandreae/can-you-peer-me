use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_lite::{AsyncRead, AsyncWrite, StreamExt};
use futures_util::{Sink, SinkExt};
use p2panda_sync::cbor::{into_cbor_sink, into_cbor_stream};
use p2panda_sync::{FromSync, SyncError, SyncProtocol};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::AppTopic;

#[derive(Debug, Serialize, Deserialize)]
enum SyncMessage {
    TopicQuery(AppTopic),
    Done,
}

/// A sync implementation which fulfills basic protocol requirements but nothing more
#[derive(Debug)]
pub struct DummyProtocol {}

#[async_trait]
impl<'a> SyncProtocol<'a, AppTopic> for DummyProtocol {
    fn name(&self) -> &'static str {
        static PROTOCOL_NAME: &str = "dummy_protocol_v1";
        PROTOCOL_NAME
    }
    async fn initiate(
        self: Arc<Self>,
        topic_query: AppTopic,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<&'a mut (dyn Sink<FromSync<AppTopic>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        sink.send(SyncMessage::TopicQuery(topic_query.clone()))
            .await?;

        // Wait a few seconds to simulate some very intensive sync process.
        sleep(Duration::from_secs(3)).await;

        sink.send(SyncMessage::Done).await?;
        app_tx.send(FromSync::HandshakeSuccess(topic_query)).await?;

        while let Some(result) = stream.next().await {
            let message: SyncMessage = result?;
            match &message {
                SyncMessage::TopicQuery(_) => panic!(),
                SyncMessage::Done => break,
            }
        }

        sink.flush().await?;
        app_tx.flush().await?;

        Ok(())
    }

    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<&'a mut (dyn Sink<FromSync<AppTopic>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        while let Some(result) = stream.next().await {
            let message: SyncMessage = result?;
            match &message {
                SyncMessage::TopicQuery(topic_query) => {
                    app_tx
                        .send(FromSync::HandshakeSuccess(topic_query.clone()))
                        .await?
                }
                SyncMessage::Done => break,
            }
        }

        sink.send(SyncMessage::Done).await?;

        sink.flush().await?;
        app_tx.flush().await?;

        Ok(())
    }
}
