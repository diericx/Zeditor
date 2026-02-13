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

    #[error("nothing to undo")]
    NothingToUndo,

    #[error("nothing to redo")]
    NothingToRedo,

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
