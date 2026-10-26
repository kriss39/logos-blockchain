use std::{pin::pin, sync::Arc};

use futures::{StreamExt as _, channel::mpsc, stream};
use lb_log_targets::cryptarchia;
use libp2p::{PeerId, StreamProtocol};
use libp2p_stream::Control;
use tokio::{
    sync::{Semaphore, oneshot},
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

/// The maximum number of block bytes read ahead of the consumer of a
/// download. Reading ahead lets the providing peer finish the batch, and free
/// its download slot, while blocks are still being applied.
const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024 * 1024;

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

    pub async fn receive_blocks(
        request_stream: BlocksRequestStream,
        timeout: Duration,
    ) -> Result<(), ChainSyncError> {
        let libp2p_stream = request_stream.stream;
        let peer_id = request_stream.peer_id;
        let reply_channel = request_stream.reply_channel;

        #[expect(
            closure_returning_async_block,
            reason = "Signature expected by `try_unfold`"
        )]
        let stream = stream::try_unfold(libp2p_stream, move |mut stream| async move {
            let response = time::timeout(timeout, unpack_from_reader(&mut stream)).await;

            match response {
                Ok(Ok(DownloadBlocksResponse::Block(block))) => Ok(Some((block, stream))),
                Ok(Ok(DownloadBlocksResponse::NoMoreBlocks)) => {
                    utils::close_stream(peer_id, stream).await?;
                    Ok(None)
                }
                Ok(Ok(DownloadBlocksResponse::Failure(reason))) => {
                    utils::close_stream(peer_id, stream).await?;

                    Err(ChainSyncError {
                        peer: peer_id,
                        kind: ChainSyncErrorKind::BlockProviderUnavailable(reason),
                    })
                }
                Ok(Err(e)) => {
                    drop(utils::close_stream(peer_id, stream).await);
                    Err(ChainSyncError::from((peer_id, e)))
                }
                Err(e) => {
                    drop(utils::close_stream(peer_id, stream).await);
                    Err(ChainSyncError::from((peer_id, e)))
                }
            }
        });

        // Each buffered block holds permits for its bytes until it is taken.
        let (sender, receiver) = mpsc::unbounded();
        let boxed_stream: BoxedStream<_> = Box::new(receiver.map(|(block, _permit)| block));
        reply_channel
            .send(boxed_stream)
            .map_err(|_| ChainSyncError {
                peer: peer_id,
                kind: ChainSyncErrorKind::ChannelSendError(
                    "Failed to send blocks stream".to_owned(),
                ),
            })?;

        let buffer = Arc::new(Semaphore::new(DOWNLOAD_BUFFER_BYTES));
        let mut stream = pin!(stream);
        while let Some(block) = stream.next().await {
            let bytes = block.as_ref().map_or(1, SerialisedBlock::len);
            let permits = u32::try_from(bytes.clamp(1, DOWNLOAD_BUFFER_BYTES))
                .expect("buffer size fits in u32");
            let permit = Arc::clone(&buffer)
                .acquire_many_owned(permits)
                .await
                .expect("download buffer is never closed");
            if sender.unbounded_send((block, permit)).is_err() {
                // The consumer dropped the stream of blocks.
                break;
            }
        }
        Ok(())
    }
}
