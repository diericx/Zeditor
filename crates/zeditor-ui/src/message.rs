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

/// Payload describing what is being dragged. Extensible for future drag sources.
#[derive(Debug, Clone)]
pub enum DragPayload {
    SourceAsset {
        asset_id: Uuid,
        thumbnail: Option<iced::widget::image::Handle>,
        name: String,
    },
}

/// Preview state for a source asset being dragged over the timeline.
#[derive(Debug, Clone)]
pub struct SourceDragPreview {
    pub asset_id: Uuid,
    pub duration_secs: f64,
    pub track_index: usize,
    pub position: TimelinePosition,
}

/// App-level drag state tracking.
#[derive(Debug, Clone)]
pub struct DragState {
    pub payload: DragPayload,
    pub cursor_position: iced::Point,
    pub over_timeline: bool,
    pub timeline_track: Option<usize>,
    pub timeline_position: Option<TimelinePosition>,
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

    // Thumbnails
    ThumbnailGenerated {
        asset_id: Uuid,
        result: Result<(Vec<u8>, u32, u32), String>,
    },

    // Source card hover
    SourceCardHovered(Option<Uuid>),

    // Drag from source
    StartDragFromSource(Uuid),
    DragMoved(iced::Point),
    DragReleased,
    DragEnteredTimeline,
    DragExitedTimeline,
    DragOverTimeline(iced::Point),

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
