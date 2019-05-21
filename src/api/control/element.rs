//! Element definitions and implementations.

use serde::{
    de::{self, Deserializer, Error, Visitor},
    Deserialize,
};
use std::fmt;

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind")]
pub enum Element {
    WebRtcPublishEndpoint { spec: WebRtcPublishEndpoint },
    WebRtcPlayEndpoint { spec: WebRtcPlayEndpoint },
}

#[derive(Deserialize, Debug, Clone)]
/// Peer-to-peer mode of [`WebRtcPublishEndpoint`].
pub enum P2pMode {
    /// Always connect peer-to-peer.
    Always,
}

#[derive(Deserialize, Debug, Clone)]
/// Media element which is able to play media data for client via WebRTC.
pub struct WebRtcPublishEndpoint {
    /// Peer-to-peer mode.
    pub p2p: P2pMode,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WebRtcPlayEndpoint {
    pub src: LocalUri,
}

#[derive(Debug, Clone)]
pub struct LocalUri {
    pub room_id: String,
    pub member_id: String,
    pub pipeline_id: String,
}

impl<'de> Deserialize<'de> for LocalUri {
    fn deserialize<D>(deserializer: D) -> Result<LocalUri, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LocalUriVisitor;

        impl<'de> Visitor<'de> for LocalUriVisitor {
            type Value = LocalUri;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "Uri in format local://room_id/member_id/pipeline_id",
                )
            }

            // TODO: Return error with information about place where this error
            //       happened.
            fn visit_str<E>(self, value: &str) -> Result<LocalUri, E>
            where
                E: de::Error,
            {
                let protocol_name: String = value.chars().take(8).collect();
                if protocol_name != "local://" {
                    return Err(Error::custom(
                        "Expected local uri in format \
                         local://room_id/member_id/pipeline_id!",
                    ));
                }

                let uri_body = value.chars().skip(8).collect::<String>();
                let mut uri_body_splitted: Vec<&str> =
                    uri_body.rsplit('/').collect();
                let uri_body_splitted_len = uri_body_splitted.len();
                if uri_body_splitted_len != 3 {
                    let error_msg = if uri_body_splitted_len == 0 {
                        "Missing room_id, member_id, pipeline_id"
                    } else if uri_body_splitted_len == 1 {
                        "Missing member_id, pipeline_id"
                    } else if uri_body_splitted_len == 2 {
                        "Missing pipeline_id"
                    } else {
                        "Too many params"
                    };
                    return Err(Error::custom(error_msg));
                }
                let room_id = uri_body_splitted.pop().unwrap().to_string();
                if room_id.is_empty() {
                    return Err(Error::custom("room_id is empty!"));
                }
                let member_id = uri_body_splitted.pop().unwrap().to_string();
                if member_id.is_empty() {
                    return Err(Error::custom("member_id is empty!"));
                }
                let pipeline_id = uri_body_splitted.pop().unwrap().to_string();
                if pipeline_id.is_empty() {
                    return Err(Error::custom("pipeline_id is empty!"));
                }

                Ok(LocalUri {
                    room_id,
                    member_id,
                    pipeline_id,
                })
            }
        }

        deserializer.deserialize_identifier(LocalUriVisitor)
    }
}
