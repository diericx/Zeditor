use std::path::PathBuf;

use uuid::Uuid;
use zeditor_core::timeline::TimelinePosition;

#[derive(Debug, Clone)]
pub enum Message {
    // Source library
    ImportMedia(PathBuf),
    MediaImported(Result<zeditor_core::media::MediaAsset, String>),
    RemoveAsset(Uuid),

    // Timeline
    AddClipToTimeline {
        asset_id: Uuid,
        track_index: usize,
        position: TimelinePosition,
    },
    MoveClip {
        source_track: usize,
        clip_id: Uuid,
        dest_track: usize,
        position: TimelinePosition,
    },
    CutClip {
        track_index: usize,
        position: TimelinePosition,
    },
    ResizeClip {
        track_index: usize,
        clip_id: Uuid,
        new_end: TimelinePosition,
    },

    // Playback
    Play,
    Pause,
    SeekTo(TimelinePosition),

    // Project
    Undo,
    Redo,
    SaveProject,
    LoadProject(PathBuf),
}
