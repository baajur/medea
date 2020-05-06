//! [`Observable`] implementation of the [`TrackSnapshotAccessor`] which will be
//! used in the Jason for the `Track`'s real state updating.

use futures::Stream;
use medea_client_api_proto::{
    snapshots::{track::TrackSnapshotAccessor, TrackSnapshot},
    Direction, MediaType, TrackId,
};
use medea_reactive::ObservableCell;

/// Reactive snapshot of the state for the `MediaTrack`.
#[derive(Debug)]
pub struct ObservableTrackSnapshot {
    /// ID of this [`ObservableTrackSnapshot`].
    pub id: TrackId,

    /// If `true` then `MediaTrack` is muted.
    pub is_muted: ObservableCell<bool>,

    /// Direction of `MediaTrack`.
    pub direction: Direction,

    /// Media type of `MediaTrack`.
    pub media_type: MediaType,

    /// `MediaTrack` state change which was requested by a user while RPC
    /// reconnection.
    pub intent: TrackIntent,
}

#[derive(Debug, Default)]
pub struct TrackIntent {
    /// `MediaTrack`'s mute state which user intents.
    pub is_muted: Option<bool>,
}

impl TrackIntent {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ObservableTrackSnapshot {
    /// Returns [`Stream`] to which will be sent
    /// [`ObservableTrackSnapshot::is_muted`] update.
    pub fn on_track_update(&self) -> impl Stream<Item = bool> {
        self.is_muted.subscribe()
    }

    /// Returns direction of this `MediaTrack`.
    pub fn get_direction(&self) -> &Direction {
        &self.direction
    }

    /// Returns media type of this `MediaTrack`.
    pub fn get_media_type(&self) -> MediaType {
        self.media_type
    }

    /// Returns mute state of this `MediaTrack`.
    pub fn get_is_muted(&self) -> bool {
        self.is_muted.get()
    }

    /// Returns [`TrackId`] of this [`ObservableTrackSnapshot`].
    pub fn get_id(&self) -> TrackId {
        self.id
    }
}

impl TrackSnapshotAccessor for ObservableTrackSnapshot {
    fn new(
        id: TrackId,
        is_muted: bool,
        direction: Direction,
        media_type: MediaType,
    ) -> Self {
        Self {
            id,
            is_muted: ObservableCell::new(is_muted),
            direction,
            media_type,
            intent: TrackIntent::new(),
        }
    }

    fn set_is_muted(&mut self, is_muted: bool) {
        self.is_muted.set(is_muted);
    }
}

impl From<&ObservableTrackSnapshot> for TrackSnapshot {
    fn from(from: &ObservableTrackSnapshot) -> Self {
        TrackSnapshot {
            id: from.id,
            media_type: from.media_type,
            direction: from.direction.clone(),
            is_muted: from.is_muted.get(),
        }
    }
}