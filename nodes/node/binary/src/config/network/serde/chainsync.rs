use core::time::Duration;
use std::num::NonZeroUsize;

use lb_libp2p::cryptarchia_sync::config::{
    DEFAULT_DOWNLOAD_BUFFER_BYTES, DEFAULT_PROVIDER_SEND_TIMEOUT,
};
use lb_utils::bounded_duration::{MILLI, MinimalBoundedDuration};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The maximum duration to wait for a peer to respond
    /// with a message.
    #[serde_as(as = "MinimalBoundedDuration<1, MILLI>")]
    pub peer_response_timeout: Duration,
    /// The maximum number of inbound block download requests that can be
    /// served concurrently. The same limit separately applies to inbound
    /// requests whose message is still being read. Tip requests are not
    /// counted against the download limit.
    pub max_inbound_requests: NonZeroUsize,
    /// The maximum duration a single write to a downloading peer may block
    /// before the download is aborted, freeing its download slot.
    #[serde_as(as = "MinimalBoundedDuration<1, MILLI>")]
    pub provider_send_timeout: Duration,
    /// The maximum number of block bytes read ahead of block processing
    /// during a local download, so the providing peer can finish the batch
    /// early.
    pub download_buffer_bytes: NonZeroUsize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            peer_response_timeout: Duration::from_secs(5),
            max_inbound_requests: 10.try_into().unwrap(),
            provider_send_timeout: DEFAULT_PROVIDER_SEND_TIMEOUT,
            download_buffer_bytes: DEFAULT_DOWNLOAD_BUFFER_BYTES,
        }
    }
}
