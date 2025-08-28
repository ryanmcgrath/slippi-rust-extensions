use std::net::UdpSocket;
use std::ops::{Deref, DerefMut};

use rusty_enet::{Event, Host};
use thiserror::Error;

/// Any error that can occur during a `EnetClient::receive()` call.
#[derive(Debug, Error)]
pub enum ReceiveError {
    #[error(transparent)]
    HostRead(std::io::Error),

    #[error(transparent)]
    Deserialize(serde_json::Error),

    #[error("Matchmaking server disconnected")]
    Disconnect,

    #[error("No response from matchmaking server")]
    Timeout,

    #[error(transparent)]
    Utf8Read(std::str::Utf8Error)
}

/// A wrapper around a `rusty_enet::Host`. We provide a few additional methods
/// via this wrapper, but also deref to the host itself - so you can simply call
/// any method from `rusty_enet::Host` on this.
#[derive(Debug)]
pub struct EnetClient(Host<UdpSocket>);

impl EnetClient {
    /// Wraps a host and returns it.
    pub fn new(host: Host<UdpSocket>) -> Self {
        Self(host)
    }

    /// Repeatedly checks the inner socket for new data. We will attempt to deserialize any data
    /// received to our expected type.
    ///
    /// This attempts to replicate the timeout handling of the C++ version, albeit against what
    /// appears to be a newer/different enet API. For the way this is called, it's not a
    /// significant burden to just chunk the timeout checking manually 
    /// (e.g 5000ms in 250ms chunks, etc).
    pub fn receive<T>(&mut self, mut timeout_ms: i32) -> Result<T, ReceiveError>
    where
        T: serde::de::DeserializeOwned,
    {
        let host_service_timeout_ms = 250;

        // Make sure loop runs at least once
        if timeout_ms < host_service_timeout_ms {
            timeout_ms = host_service_timeout_ms;
        }

        // This is not a perfect way to timeout but hopefully it's close enough?
        let max_attempts = timeout_ms / host_service_timeout_ms;
        
        let mut attempt = 0;

        while attempt < max_attempts {
            if let Some(event) = self.0.service().map_err(ReceiveError::HostRead)? {
                if let Event::Disconnect { .. } = event {
                    return Err(ReceiveError::Disconnect);
                }

                if let Event::Receive { peer: _, channel_id: _, packet } = event {
                    let message = str::from_utf8(packet.data()).map_err(ReceiveError::Utf8Read)?;
                    let data = serde_json::from_str(message).map_err(ReceiveError::Deserialize)?;
                    return Ok(data);
                }
            }

            attempt += 1;
            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        Err(ReceiveError::Timeout)
    }

    /// Attempts to terminate the connection by gracefully disconnecting peers. If peers
    /// do not appear to disconnect, this will force disconnects after around 3000ms.
    pub fn terminate(mut self) {
        for peer in self.0.peers_mut() {
            peer.disconnect(0);
        }

        let timeout = 3000;
        let mut slept = 0;

        while slept <= timeout {
            // If we receive a Disconnect, then we can bail early and let the `Drop` impl
            // on `Host` handle cleaning up resources.
            if let Ok(Some(Event::Disconnect { peer: _, data: _ })) = self.0.service() {
                return;
            }

            std::thread::sleep(std::time::Duration::from_millis(250));
            slept += 250;
        }

        // If we didn't receive a Disconnect event, then we need to force disconnect
        // everything. When the `host` is dropped at the end of this function it will
        // trigger `enet_destroy` behind the scenes.
        for peer in self.0.peers_mut() {
            peer.reset();
        }
    }
}

impl Deref for EnetClient {
    type Target = Host<UdpSocket>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EnetClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
