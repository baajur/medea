//! Snapshot for the `Room` object.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{EventHandler, IceCandidate, IceServer, PeerId, Track, TrackPatch};

use super::{PeerSnapshot, PeerSnapshotAccessor, TrackSnapshotAccessor};

/// Snapshot of the state for the `Room`.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
pub struct RoomSnapshot {
    /// All `Peer`s of this `Room`.
    pub peers: HashMap<PeerId, PeerSnapshot>,
}

impl RoomSnapshot {
    /// Returns new empty [`RoomSnapshot`].
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for RoomSnapshot {
    fn default() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }
}

pub trait RoomSnapshotAccessor {
    type Peer: PeerSnapshotAccessor;

    /// Inserts new `Peer` to this `Room`.
    fn insert_peer(&mut self, peer_id: PeerId, peer: Self::Peer);

    /// Removes `Peer` from this `Room`.
    fn remove_peer(&mut self, peer_id: PeerId);

    /// Updates `Peer` with provided `peer_id`.
    ///
    /// To `update_fn` will be provided mutable reference to the
    /// [`PeerSnapshotAccessor`] with which you can update `Peer`.
    fn update_peer<F>(&mut self, peer_id: PeerId, update_fn: F)
    where
        F: FnOnce(Option<&mut Self::Peer>);

    /// Updates this `Room` by provided [`RoomSnapshot`].
    fn update_snapshot(&mut self, snapshot: RoomSnapshot) {
        for (peer_id, peer_snapshot) in snapshot.peers {
            self.update_peer(peer_id, |peer| {
                if let Some(peer) = peer {
                    peer.update_snapshot(peer_snapshot);
                }
            });
        }
    }
}

impl RoomSnapshotAccessor for RoomSnapshot {
    type Peer = PeerSnapshot;

    fn insert_peer(&mut self, peer_id: PeerId, peer: Self::Peer) {
        self.peers.insert(peer_id, peer);
    }

    fn remove_peer(&mut self, peer_id: PeerId) {
        self.peers.remove(&peer_id);
    }

    fn update_peer<F>(&mut self, peer_id: PeerId, update_fn: F)
    where
        F: FnOnce(Option<&mut Self::Peer>),
    {
        (update_fn)(self.peers.get_mut(&peer_id));
    }
}

impl<R> EventHandler for R
where
    R: RoomSnapshotAccessor,
{
    type Output = ();

    /// Creates [`PeerSnapshot`] from the provided data and inserts
    /// [`PeerSnapshot`] into this [`RoomSnapshot`].
    fn on_peer_created(
        &mut self,
        peer_id: PeerId,
        sdp_offer: Option<String>,
        tracks: Vec<Track>,
        ice_servers: HashSet<IceServer>,
        is_force_relayed: bool,
    ) {
        type Peer<R> = <R as RoomSnapshotAccessor>::Peer;
        type Track<R> = <Peer<R> as PeerSnapshotAccessor>::Track;

        let tracks = tracks
            .into_iter()
            .map(|track| {
                (
                    track.id,
                    Track::<R>::new(
                        track.id,
                        track.is_muted,
                        track.direction,
                        track.media_type,
                    ),
                )
            })
            .collect();
        let peer = R::Peer::new(
            peer_id,
            sdp_offer,
            ice_servers,
            is_force_relayed,
            tracks,
        );
        self.insert_peer(peer_id, peer);
    }

    /// Sets SDP answer of the [`PeerSnapshot`] with the provided [`PeerId`].
    fn on_sdp_answer_made(&mut self, peer_id: PeerId, sdp_answer: String) {
        self.update_peer(peer_id, move |peer| {
            if let Some(peer) = peer {
                peer.set_sdp_answer(Some(sdp_answer));
            }
        });
    }

    /// Adds provided [`IceCandidate`] into [`PeerSnapshot`] with the provided
    /// [`PeerId`].
    fn on_ice_candidate_discovered(
        &mut self,
        peer_id: PeerId,
        candidate: IceCandidate,
    ) {
        self.update_peer(peer_id, move |peer| {
            if let Some(peer) = peer {
                peer.add_ice_candidate(candidate);
            }
        });
    }

    /// Removes [`PeerSnapshot`] with the provided [`PeerId`]s from this
    /// [`RoomSnapshot`].
    fn on_peers_removed(&mut self, peer_ids: Vec<PeerId>) {
        for peer_id in peer_ids {
            self.remove_peer(peer_id);
        }
    }

    /// Updates [`TrackSnapshot`]s from the [`PeerSnapshot`] with the provided
    /// [`PeerId`] by provided [`TrackPatch`]s.
    fn on_tracks_updated(&mut self, peer_id: PeerId, tracks: Vec<TrackPatch>) {
        self.update_peer(peer_id, move |peer| {
            if let Some(peer) = peer {
                peer.update_tracks_by_patches(tracks);
            }
        });
    }

    /// Updates this [`RoomSnapshot`] with a new one.
    fn on_restore_state(&mut self, snapshot: RoomSnapshot) {
        self.update_snapshot(snapshot);
    }
}
