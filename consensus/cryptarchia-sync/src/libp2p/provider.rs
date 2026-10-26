use std::time::{Duration, Instant};

use futures::{TryStreamExt as _, stream::BoxStream};
use lb_binary_codec::bincode::BoundedSerializeOp;
use lb_log_targets::cryptarchia;
use libp2p::{PeerId, Stream as Libp2pStream};
use serde::de::DeserializeOwned;
use tokio::{sync::mpsc, time};
use tracing::{debug, error};

use crate::{
    BlocksResponse, BlocksUnavailableReason, DynError, ProviderResponse, TipResponse,
    libp2p::{
        errors::{ChainSyncError, ChainSyncErrorKind},
        messages::{DownloadBlocksResponse, RequestMessage},
        packing::unpack_from_reader,
        utils::{close_stream, send_message},
    },
    messages::{GetTipResponse, GetTipResponseReason, SerialisedBlock},
};

const LOG_TARGET: &str = cryptarchia::sync::libp2p::PROVIDER;

pub const MAX_ADDITIONAL_BLOCKS: usize = 5;

pub struct Provider;

pub type ReceivingRequestStream = (PeerId, Libp2pStream, RequestMessage);

impl Provider {
    pub async fn process_request(
        peer_id: PeerId,
        mut stream: Libp2pStream,
        timeout: Duration,
    ) -> Result<ReceivingRequestStream, ChainSyncError> {
        let request: RequestMessage = time::timeout(timeout, unpack_from_reader(&mut stream))
            .await
            .map_err(|e| ChainSyncError::from((peer_id, e)))?
            .map_err(|e| ChainSyncError::from((peer_id, e)))?;

        Ok((peer_id, stream, request))
    }

    pub async fn provide_tip(
        mut reply_receiver: mpsc::Receiver<TipResponse>,
        peer_id: PeerId,
        mut libp2p_stream: Libp2pStream,
    ) -> Result<(), ChainSyncError> {
        let response = reply_receiver.recv().await.unwrap_or_else(|| {
            debug!(target: LOG_TARGET, %peer_id, "no tip provider answered; replying unknown error");
            ProviderResponse::Unavailable {
                reason: GetTipResponseReason::Unknown,
            }
        });

        let send_result = match response {
            ProviderResponse::Available(tip) => {
                send_message(peer_id, &mut libp2p_stream, &tip).await
            }
            ProviderResponse::Unavailable { reason } => {
                let response = GetTipResponse::Failure(reason);
                send_message(peer_id, &mut libp2p_stream, &response).await
            }
        };

        drop(close_stream(peer_id, libp2p_stream).await);

        send_result
    }

    pub async fn provide_blocks(
        mut reply_receiver: mpsc::Receiver<BlocksResponse>,
        peer_id: PeerId,
        mut libp2p_stream: Libp2pStream,
        send_timeout: Duration,
    ) -> Result<(), ChainSyncError> {
        let response = reply_receiver.recv().await.unwrap_or_else(|| {
            debug!(target: LOG_TARGET, %peer_id, "no block provider answered; replying unknown error");
            ProviderResponse::Unavailable {
                reason: BlocksUnavailableReason::Unknown,
            }
        });

        match response {
            ProviderResponse::Available(stream) => {
                Self::send_blocks(peer_id, stream, libp2p_stream, send_timeout).await
            }
            ProviderResponse::Unavailable { reason } => {
                let response = DownloadBlocksResponse::Failure(reason);
                send_message_with_timeout(peer_id, &mut libp2p_stream, &response, send_timeout)
                    .await?;
                drop(close_stream_with_timeout(peer_id, libp2p_stream, send_timeout).await);
                Ok(())
            }
        }
    }

    /// Sends every block of `stream` to the peer.
    ///
    /// Each write is bounded by `send_timeout`, so a peer that stops reading
    /// cannot hold the download slot indefinitely.
    async fn send_blocks(
        peer_id: PeerId,
        stream: BoxStream<'static, Result<SerialisedBlock, DynError>>,
        mut libp2p_stream: Libp2pStream,
        send_timeout: Duration,
    ) -> Result<(), ChainSyncError> {
        let started_at = Instant::now();

        let result = stream
            .map_err(|e| ChainSyncError {
                peer: peer_id,
                kind: ChainSyncErrorKind::ReceivingBlocksError(format!(
                    "Failed to receive block from stream: {e}"
                )),
            })
            .try_fold(
                (&mut libp2p_stream, 0usize),
                async |(stream, blocks_sent), block| {
                    let message = DownloadBlocksResponse::Block(block);
                    send_message_with_timeout(peer_id, stream, &message, send_timeout).await?;
                    Ok((stream, blocks_sent.saturating_add(1)))
                },
            )
            .await;

        let blocks_sent = result.as_ref().map_or(0, |(_, blocks_sent)| *blocks_sent);
        let final_result = match result {
            Ok(_) => {
                let message = DownloadBlocksResponse::NoMoreBlocks;
                send_message_with_timeout(peer_id, &mut libp2p_stream, &message, send_timeout).await
            }
            Err(e) => {
                error!(target: LOG_TARGET, "Failed to send blocks to peer {}: {}", peer_id, e);
                let message = DownloadBlocksResponse::Failure(BlocksUnavailableReason::Unknown);
                drop(
                    send_message_with_timeout(peer_id, &mut libp2p_stream, &message, send_timeout)
                        .await,
                );
                Err(e)
            }
        };

        drop(close_stream_with_timeout(peer_id, libp2p_stream, send_timeout).await);

        debug!(
            target: LOG_TARGET,
            %peer_id,
            blocks_sent,
            elapsed_ms = started_at.elapsed().as_millis(),
            success = final_result.is_ok(),
            "Finished serving block download"
        );

        final_result
    }
}

async fn send_message_with_timeout<M: BoundedSerializeOp + DeserializeOwned + Sync>(
    peer_id: PeerId,
    stream: &mut Libp2pStream,
    message: &M,
    timeout: Duration,
) -> Result<(), ChainSyncError> {
    time::timeout(timeout, send_message(peer_id, stream, message))
        .await
        .map_err(|e| ChainSyncError::from((peer_id, e)))?
}

async fn close_stream_with_timeout(
    peer_id: PeerId,
    stream: Libp2pStream,
    timeout: Duration,
) -> Result<(), ChainSyncError> {
    time::timeout(timeout, close_stream(peer_id, stream))
        .await
        .map_err(|e| ChainSyncError::from((peer_id, e)))?
}
