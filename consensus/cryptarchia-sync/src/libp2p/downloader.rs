use std::{num::NonZeroUsize, sync::Arc};

use futures::{FutureExt as _, future::BoxFuture, stream};
use lb_log_targets::cryptarchia;
use libp2p::{PeerId, Stream as Libp2pStream, StreamProtocol};
use libp2p_stream::Control;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot},
    time,
    time::Duration,
};
use tracing::{debug, warn};

use crate::{
    DownloadBlocksRequest, GetTipResponse,
    libp2p::{
        behaviour::{BlocksRequestStream, BoxedStream, TipRequestStream},
        errors::{ChainSyncError, ChainSyncErrorKind},
        messages::{DownloadBlocksResponse, RequestMessage},
        packing::unpack_from_reader,
        utils::{self, open_stream, send_message},
    },
    messages::SerialisedBlock,
};

const LOG_TARGET: &str = cryptarchia::sync::libp2p::DOWNLOADER;

pub struct Downloader;

impl Downloader {
    pub async fn send_tip_request(
        peer_id: PeerId,
        control: &mut Control,
        protocol_name: StreamProtocol,
        reply_sender: oneshot::Sender<Result<GetTipResponse, ChainSyncError>>,
    ) -> Result<TipRequestStream, ChainSyncError> {
        let mut stream = open_stream(peer_id, control, protocol_name).await?;

        let tip_request = RequestMessage::GetTip;
        send_message(peer_id, &mut stream, &tip_request).await?;

        let request_stream = TipRequestStream::new(peer_id, stream, reply_sender);
        Ok(request_stream)
    }

    pub async fn send_download_request(
        peer_id: PeerId,
        mut control: Control,
        request: DownloadBlocksRequest,
        protocol_name: StreamProtocol,
        reply_sender: oneshot::Sender<BoxedStream<Result<SerialisedBlock, ChainSyncError>>>,
    ) -> Result<BlocksRequestStream, ChainSyncError> {
        let mut stream = open_stream(peer_id, &mut control, protocol_name).await?;

        let download_request = RequestMessage::DownloadBlocksRequest(request);

        send_message(peer_id, &mut stream, &download_request).await?;

        let request_stream = BlocksRequestStream::new(peer_id, stream, reply_sender);
        Ok(request_stream)
    }

    pub async fn receive_tip(
        request_stream: TipRequestStream,
        timeout: Duration,
    ) -> Result<(), ChainSyncError> {
        let TipRequestStream {
            mut stream,
            peer_id,
            reply_channel,
        } = request_stream;

        let response = time::timeout(
            timeout,
            unpack_from_reader::<GetTipResponse, _>(&mut stream),
        )
        .await
        .map_err(|e| {
            warn!(target: LOG_TARGET, "Timeout while receiving tip from peer {}", peer_id);
            ChainSyncError::from((peer_id, e))
        })?
        .map_err(|e| ChainSyncError::from((peer_id, e)))
        .and_then(|response| match response {
            tip @ GetTipResponse::Tip { .. } => Ok(tip),
            GetTipResponse::Failure(reason) => Err(ChainSyncError {
                peer: peer_id,
                kind: ChainSyncErrorKind::RequestTipError(reason),
            }),
        });

        if let Err(e) = reply_channel.send(response) {
            debug!(
                target: LOG_TARGET,
                "Tip requester dropped before receiving response from peer {peer_id}: {e:?}"
            );
        }

        utils::close_stream(peer_id, stream).await
    }

    /// Hands the consumer a stream of the blocks sent by the peer, and
    /// returns a future that reads them off the wire.
    ///
    /// The future reads ahead of the consumer, buffering up to
    /// `buffer_bytes` of blocks (always at least one block). This lets the
    /// peer finish sending the batch, and free its download slot, while
    /// earlier blocks are still being applied locally.
    ///
    /// The future completes when the peer ends the download, a read fails,
    /// or the consumer drops the stream.
    pub fn receive_blocks(
        request_stream: BlocksRequestStream,
        timeout: Duration,
        buffer_bytes: NonZeroUsize,
    ) -> Result<BoxFuture<'static, ()>, ChainSyncError> {
        let BlocksRequestStream {
            stream: libp2p_stream,
            peer_id,
            reply_channel,
        } = request_stream;

        // Bytes of buffered blocks are accounted by semaphore permits, which
        // travel with each block and are released once the consumer takes it.
        let buffer_bytes = buffer_bytes.get().min(Semaphore::MAX_PERMITS);
        let buffer = Arc::new(Semaphore::new(buffer_bytes));
        let (sender, receiver) = mpsc::unbounded_channel();

        let blocks = stream::unfold(receiver, async |mut receiver| {
            let (block, _permit): (_, Option<OwnedSemaphorePermit>) = receiver.recv().await?;
            Some((block, receiver))
        });

        let boxed_stream: BoxedStream<_> = Box::new(Box::pin(blocks));
        if reply_channel.send(boxed_stream).is_err() {
            return Err(ChainSyncError {
                peer: peer_id,
                kind: ChainSyncErrorKind::ChannelSendError(
                    "Failed to send blocks stream".to_owned(),
                ),
            });
        }

        Ok(Self::read_blocks(
            peer_id,
            libp2p_stream,
            timeout,
            buffer,
            buffer_bytes,
            sender,
        )
        .boxed())
    }

    async fn read_blocks(
        peer_id: PeerId,
        mut stream: Libp2pStream,
        timeout: Duration,
        buffer: Arc<Semaphore>,
        buffer_bytes: usize,
        sender: BufferedBlocksSender,
    ) {
        let outcome = Self::forward_blocks(
            peer_id,
            &mut stream,
            timeout,
            &buffer,
            buffer_bytes,
            &sender,
        )
        .await;
        let close_result = utils::close_stream(peer_id, stream).await;

        // Blocks already buffered are still delivered before any error.
        match outcome {
            ForwardOutcome::Completed => {
                if let Err(error) = close_result {
                    drop(sender.send((Err(error), None)));
                }
            }
            ForwardOutcome::Failed(error) => drop(sender.send((Err(error), None))),
            ForwardOutcome::Dropped => {
                debug!(target: LOG_TARGET, "Block download from peer {peer_id} dropped by consumer");
            }
        }
    }

    /// Reads blocks off `stream` into `sender` until the download ends.
    async fn forward_blocks(
        peer_id: PeerId,
        stream: &mut Libp2pStream,
        timeout: Duration,
        buffer: &Arc<Semaphore>,
        buffer_bytes: usize,
        sender: &BufferedBlocksSender,
    ) -> ForwardOutcome {
        loop {
            let response = tokio::select! {
                () = sender.closed() => return ForwardOutcome::Dropped,
                response = time::timeout(timeout, unpack_from_reader(&mut *stream)) => response,
            };

            let block = match response {
                Ok(Ok(DownloadBlocksResponse::Block(block))) => block,
                Ok(Ok(DownloadBlocksResponse::NoMoreBlocks)) => return ForwardOutcome::Completed,
                Ok(Ok(DownloadBlocksResponse::Failure(reason))) => {
                    return ForwardOutcome::Failed(ChainSyncError {
                        peer: peer_id,
                        kind: ChainSyncErrorKind::BlockProviderUnavailable(reason),
                    });
                }
                Ok(Err(e)) => return ForwardOutcome::Failed(ChainSyncError::from((peer_id, e))),
                Err(e) => return ForwardOutcome::Failed(ChainSyncError::from((peer_id, e))),
            };

            let permits = u32::try_from(block.len().clamp(1, buffer_bytes))
                .expect("buffer bytes are bounded by Semaphore::MAX_PERMITS");
            let permit = tokio::select! {
                () = sender.closed() => return ForwardOutcome::Dropped,
                permit = Arc::clone(buffer).acquire_many_owned(permits) => {
                    permit.expect("download buffer semaphore is never closed")
                }
            };

            if sender.send((Ok(block), Some(permit))).is_err() {
                return ForwardOutcome::Dropped;
            }
        }
    }
}

enum ForwardOutcome {
    /// The peer sent every block of the download.
    Completed,
    /// The download failed before the peer sent every block.
    Failed(ChainSyncError),
    /// The consumer dropped the stream of blocks.
    Dropped,
}

type BufferedBlocksSender = mpsc::UnboundedSender<(
    Result<SerialisedBlock, ChainSyncError>,
    Option<OwnedSemaphorePermit>,
)>;
