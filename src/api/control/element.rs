//! Element definitions and implementations.

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "kind")]

/// Media element for send media data.
pub enum PublishElement {
    /// Media element which is able to send media data to another client via WebRTC.
    WebRtcPublishEndpoint{
        spec: WebRtcPublishEndpoint,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "kind")]
/// Media element for receive media data.
pub enum PlayElement {
    /// Media element which is able to play media data for client via WebRTC.
    WebRtcPlayEndpoint
}

#[derive(Serialize, Deserialize, Debug)]
/// Peer-to-peer mode of [`WebRtcPublishEndpoint`].
pub enum P2pMode {
    /// Always connect peer-to-peer.
    Always,
}

#[derive(Serialize, Deserialize, Debug)]
/// Media element which is able to play media data for client via WebRTC.
pub struct WebRtcPublishEndpoint {
    /// Peer-to-peer mode.
    p2p: P2pMode,
}
