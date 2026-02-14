use thiserror::Error;
use uuid::Uuid;

use crate::timeline::TimelinePosition;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("clip not found: {0}")]
    ClipNotFound(Uuid),

    #[error("track not found: {0}")]
    TrackNotFound(usize),

    #[error("media asset not found: {0}")]
    AssetNotFound(Uuid),

    #[error("cut position {position:?} is outside clip bounds")]
    CutOutsideClip { position: TimelinePosition },

    #[error("clip overlap detected at timeline position {position:?}")]
    ClipOverlap { position: TimelinePosition },

    #[error("invalid time range: start {start:?} >= end {end:?}")]
    InvalidTimeRange {
        start: TimelinePosition,
        end: TimelinePosition,
    },

    #[error("no mirror track exists for track {0}")]
    NoMirrorTrack(usize),

    #[error("track type mismatch: expected {expected:?} but got {got:?}")]
    TrackTypeMismatch {
        expected: crate::timeline::TrackType,
        got: crate::timeline::TrackType,
    },

    #[error("cannot insert: track {0} is not a {1:?} track")]
    InvalidTrackInsertion(usize, crate::timeline::TrackType),

    #[error("nothing to undo")]
    NothingToUndo,

    #[error("nothing to redo")]
    NothingToRedo,

    #[error("project file version {got} is newer than this app supports (max {max})")]
    VersionTooNew { got: String, max: String },

    #[error("project file version {got} is too old (minimum supported: {min})")]
    VersionTooOld { got: String, min: String },

    #[error("invalid project file: {0}")]
    InvalidProjectFile(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
