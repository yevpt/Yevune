//! 有界输出流、逐块消费确认与对象 Range 传输。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;
use tokio::sync::{mpsc, oneshot};

use crate::storage::{ObjectStore, StorageError, STREAM_CHUNK_SIZE};

use super::{ByteStream, Error, Result};

struct Delivery {
    item: Option<Result<Bytes>>,
    acknowledged: Option<oneshot::Sender<()>>,
    finalized: Option<oneshot::Receiver<()>>,
}

/// 管线生产端；封装私有 Delivery 协议。
pub(crate) struct OutputSender(mpsc::Sender<Delivery>);

impl OutputSender {
    pub(crate) async fn closed(&self) {
        self.0.closed().await;
    }
}

struct DeliveredStream {
    rx: mpsc::Receiver<Delivery>,
    finalizing: Option<oneshot::Receiver<()>>,
    done: bool,
}

impl Stream for DeliveredStream {
    type Item = Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }
        if let Some(finalizing) = &mut self.finalizing {
            return match Pin::new(finalizing).poll(cx) {
                Poll::Ready(_) => {
                    self.done = true;
                    self.finalizing = None;
                    Poll::Ready(None)
                }
                Poll::Pending => Poll::Pending,
            };
        }
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(mut delivery)) => {
                if let Some(acknowledged) = delivery.acknowledged.take() {
                    let _ = acknowledged.send(());
                }
                match delivery.item {
                    Some(item) => Poll::Ready(Some(item)),
                    None => {
                        self.finalizing = delivery.finalized.take();
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            Poll::Ready(None) => {
                self.done = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub(crate) fn channel() -> (OutputSender, ByteStream) {
    let (tx, rx) = mpsc::channel(2);
    (
        OutputSender(tx),
        Box::pin(DeliveredStream {
            rx,
            finalizing: None,
            done: false,
        }),
    )
}

pub(crate) fn object_stream(store: Arc<dyn ObjectStore>, key: String, size: u64) -> ByteStream {
    let (tx, output) = channel();
    tokio::spawn(async move {
        if let Err(error) = pump_object(store, key, size, &tx).await {
            send_error(&tx, error).await;
        }
    });
    output
}

pub(crate) async fn pump_object(
    store: Arc<dyn ObjectStore>,
    key: String,
    size: u64,
    tx: &OutputSender,
) -> Result<()> {
    let mut offset = 0_u64;
    while offset < size {
        let end = (offset + STREAM_CHUNK_SIZE as u64).min(size);
        let bytes = match store.get_range(&key, offset..end).await {
            Ok(bytes) if !bytes.is_empty() => bytes,
            Ok(_) => {
                return Err(Error::Storage(StorageError::Backend(format!(
                    "对象 {key} 在 {offset} 字节提前结束"
                ))))
            }
            Err(error) => return Err(error.into()),
        };
        offset += bytes.len() as u64;
        if send_chunk(tx, bytes).await.is_err() {
            return Ok(());
        }
    }
    Ok(())
}

pub(crate) async fn send_chunk(tx: &OutputSender, bytes: Bytes) -> std::result::Result<(), ()> {
    let (acknowledged, received) = oneshot::channel();
    tx.0.send(Delivery {
        item: Some(Ok(bytes)),
        acknowledged: Some(acknowledged),
        finalized: None,
    })
    .await
    .map_err(|_| ())?;
    received.await.map_err(|_| ())
}

pub(crate) async fn send_terminal(
    tx: &OutputSender,
) -> std::result::Result<oneshot::Sender<()>, ()> {
    let (acknowledged, observed) = oneshot::channel();
    let (finalized, finalizing) = oneshot::channel();
    tx.0.send(Delivery {
        item: None,
        acknowledged: Some(acknowledged),
        finalized: Some(finalizing),
    })
    .await
    .map_err(|_| ())?;
    observed.await.map_err(|_| ())?;
    Ok(finalized)
}

pub(crate) async fn send_error(tx: &OutputSender, error: Error) {
    let _ =
        tx.0.send(Delivery {
            item: Some(Err(error)),
            acknowledged: None,
            finalized: None,
        })
        .await;
}
