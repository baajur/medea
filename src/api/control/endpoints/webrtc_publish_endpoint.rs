//! `WebRtcPublishEndpoint` [Control API]'s element implementation.
//!
//! [Control API]: https://tinyurl.com/yxsqplq7

use std::convert::TryFrom;

use derive_more::{Display, From, Into};
use medea_control_api_proto::grpc::api as proto;
use serde::Deserialize;

use crate::api::control::{
    callback::url::CallbackUrl, TryFromProtobufError, Unvalidated, Validated,
    ValidationError,
};

/// ID of [`WebRtcPublishEndpoint`].
#[derive(
    Clone, Debug, Deserialize, Display, Eq, Hash, PartialEq, From, Into,
)]
pub struct WebRtcPublishId(String);

/// Peer-to-peer mode of [`WebRtcPublishEndpoint`].
#[derive(Clone, Copy, Deserialize, Debug)]
pub enum P2pMode {
    /// Always connect peer-to-peer.
    Always,

    /// Never connect peer-to-peer.
    Never,

    /// Connect peer-to-peer if it possible.
    IfPossible,
}

impl From<proto::web_rtc_publish_endpoint::P2p> for P2pMode {
    fn from(value: proto::web_rtc_publish_endpoint::P2p) -> Self {
        use proto::web_rtc_publish_endpoint::P2p::*;
        match value {
            Always => Self::Always,
            IfPossible => Self::IfPossible,
            Never => Self::Never,
        }
    }
}

impl Into<proto::web_rtc_publish_endpoint::P2p> for P2pMode {
    fn into(self) -> proto::web_rtc_publish_endpoint::P2p {
        use proto::web_rtc_publish_endpoint::P2p::*;
        match self {
            Self::Always => Always,
            Self::IfPossible => IfPossible,
            Self::Never => Never,
        }
    }
}

/// Media element which is able to publish media data for another client via
/// WebRTC.
#[derive(Clone, Deserialize, Debug)]
pub struct WebRtcPublishEndpoint<T> {
    /// Peer-to-peer mode of this [`WebRtcPublishEndpoint`].
    pub p2p: P2pMode,

    /// Option to relay all media through a TURN server forcibly.
    #[serde(default)]
    pub force_relay: bool,

    /// URL to which `OnStart` Control API callback will be sent.
    pub on_start: Option<CallbackUrl>,

    /// URL to which `OnStop` Control API callback will be sent.
    pub on_stop: Option<CallbackUrl>,

    /// Validation state of the [`WebRtcPublishEndpoint`].
    ///
    /// Can be [`Validated`] or [`Unvalidated`].
    ///
    /// [`serde`] will deserialize [`WebRtcPublishEndpoint`] into
    /// [`Unvalidated`] state. Converting from the gRPC's DTOs will cause
    /// the same behavior.
    ///
    /// To use [`WebRtcPunblishEndpoint`] you should call
    /// [`WebRtcPunblishEndpoint::validate`].
    #[serde(skip)]
    _validation_state: T,
}

impl WebRtcPublishEndpoint<Unvalidated> {
    /// Validates this [`WebRtcPublishEndpoint`].
    ///
    /// # Errors
    ///
    /// 1. Returns [`ValidationError::ForceRelayShouldBeEnabled`] if
    ///    [`WebRtcPublishEndpoint::on_start`] or [`WebRtcPublishEndpoint::
    ///    on_stop`] is set, but [`WebRtcPlayEndpoint::force_relay`] is set to
    ///    `false`.
    pub fn validate(
        self,
    ) -> Result<WebRtcPublishEndpoint<Validated>, ValidationError> {
        Ok(WebRtcPublishEndpoint {
            on_start: self.on_start,
            on_stop: self.on_stop,
            force_relay: self.force_relay,
            p2p: self.p2p,
            _validation_state: Validated,
        })
    }
}

impl TryFrom<&proto::WebRtcPublishEndpoint>
    for WebRtcPublishEndpoint<Validated>
{
    type Error = TryFromProtobufError;

    fn try_from(
        value: &proto::WebRtcPublishEndpoint,
    ) -> Result<Self, Self::Error> {
        let on_start = Some(value.on_start.clone())
            .filter(|s| !s.is_empty())
            .map(CallbackUrl::try_from)
            .transpose()?;
        let on_stop = Some(value.on_stop.clone())
            .filter(|s| !s.is_empty())
            .map(CallbackUrl::try_from)
            .transpose()?;

        let unvalidated = WebRtcPublishEndpoint {
            p2p: P2pMode::from(
                proto::web_rtc_publish_endpoint::P2p::from_i32(value.p2p)
                    .unwrap_or_default(),
            ),
            force_relay: value.force_relay,
            on_start,
            on_stop,
            _validation_state: Unvalidated,
        };

        Ok(unvalidated.validate()?)
    }
}
