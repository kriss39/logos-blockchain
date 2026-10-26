use std::{num::NonZeroUsize, time::Duration};

use serde::{Deserialize, Serialize};
use serde_with::{DurationMilliSeconds, serde_as};

/// Default for [`Config::provider_send_timeout`].
pub const DEFAULT_PROVIDER_SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// Default for [`Config::download_buffer_bytes`]: 64 MiB.
pub const DEFAULT_DOWNLOAD_BUFFER_BYTES: NonZeroUsize =
    NonZeroUsize::new(64 * 1024 * 1024).unwrap();

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The maximum duration to wait for a peer to respond
    /// with a message.
    #[serde_as(as = "DurationMilliSeconds<u64>")]
    pub peer_response_timeout: Duration,
    /// The maximum number of inbound block download requests that can be
    /// served concurrently. The same limit separately applies to inbound
    /// requests whose message is still being read. Tip requests are not
    /// counted against the download limit.
    pub max_inbound_requests: NonZeroUsize,
    /// The maximum duration a single write to a downloading peer may block
    /// before the download is aborted. A peer that stops reading would
    /// otherwise hold one of the `max_inbound_requests` slots indefinitely.
    #[serde_as(as = "DurationMilliSeconds<u64>")]
    pub provider_send_timeout: Duration,
    /// The maximum number of block bytes read ahead of the consumer of a
    /// local download. Reading ahead lets the providing peer finish the
    /// batch, and free its download slot, without waiting for every block
    /// to be applied. At least one block is always buffered.
    pub download_buffer_bytes: NonZeroUsize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            peer_response_timeout: Duration::from_secs(5),
            max_inbound_requests: NonZeroUsize::new(10).unwrap(),
            provider_send_timeout: DEFAULT_PROVIDER_SEND_TIMEOUT,
            download_buffer_bytes: DEFAULT_DOWNLOAD_BUFFER_BYTES,
        }
    }
}
