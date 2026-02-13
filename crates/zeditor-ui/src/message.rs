use std::path::PathBuf;

use uuid::Uuid;
use zeditor_core::timeline::TimelinePosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolMode {
    #[default]
    Arrow,
    Blade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    File,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    NewProject,
    LoadProject,
    Save,
    Exit,
    Undo,
    Redo,
}

#[derive(Debug, Clone)]
pub enum Message {
    // Source library
    ImportMedia(PathBuf),
    MediaImported(Result<zeditor_core::media::MediaAsset, String>),
    RemoveAsset(Uuid),
    OpenFileDialog,
    FileDialogResult(Vec<PathBuf>),
    SelectSourceAsset(Option<Uuid>),

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
    TimelineClickEmpty(TimelinePosition),
    PlaceSelectedClip {
        asset_id: Uuid,
        track_index: usize,
        position: TimelinePosition,
    },

    // Timeline view
    TimelineZoom { delta: f32, cursor_secs: f64 },
    TimelineScroll(f32),

    // Playback
    Play,
    Pause,
    SeekTo(TimelinePosition),
    TogglePlayback,
    PlaybackTick,

    // Keyboard
    KeyboardEvent(iced::keyboard::Event),

    // Project
    Undo,
    Redo,
    SaveProject,
    LoadProject(PathBuf),

    // Menu
    MenuButtonClicked(MenuId),
    MenuButtonHovered(MenuId),
    CloseMenu,
    MenuAction(MenuAction),
    Exit,
}
