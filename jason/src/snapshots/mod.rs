//! Observable implementations for the [`medea_client_api_proto::snapshots`]
//! which will be used in Jason for real state updating.

mod peer;
mod room;
mod track;

pub use peer::ObservablePeerSnapshot;
pub use room::ObservableRoomSnapshot;
pub use track::ObservableTrackSnapshot;
