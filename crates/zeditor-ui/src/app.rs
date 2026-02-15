use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use std::collections::HashMap;

use iced::widget::{button, center, column, container, image, mouse_area, opaque, row, scrollable, slider, stack, text, text_input, Space};
use iced::{event, keyboard, mouse, time, window, Background, Border, Color, Element, Event, Length, Padding, Point, Subscription, Task};
use uuid::Uuid;

use zeditor_core::effects::{EffectInstance, EffectType};
use zeditor_core::pipeline::{self, EffectContext, EffectRegistry, FrameBuffer};
use zeditor_core::project::Project;
use zeditor_core::timeline::{Clip, TimeRange, TimelinePosition, TrackType};

use crate::audio_player::AudioPlayer;
use crate::message::{ConfirmAction, ConfirmDialog, DragPayload, DragState, LeftPanelTab, MenuAction, MenuId, Message, SourceDragPreview, ToolMode, TrackContextMenu};
use crate::widgets::timeline_canvas::TimelineCanvas;

/// Preview resolution cap. 4K frames are scaled down to this for display.
const PREVIEW_MAX_WIDTH: u32 = 960;
const PREVIEW_MAX_HEIGHT: u32 = 540;

/// Info about a single clip to decode for multi-clip compositing.
#[derive(Clone, Debug)]
struct ClipDecodeInfo {
    path: PathBuf,
    time: f64,
    effects: Vec<EffectInstance>,
}

/// Request sent from UI to the decode thread.
enum DecodeRequest {
    /// Seek multiple clips for compositing. Clips are ordered bottom-to-top (V1 first).
    SeekMulti {
        clips: Vec<ClipDecodeInfo>,
        continuous: bool,
        canvas_w: u32,
        canvas_h: u32,
    },
    Stop,
}

/// Info about a single audio clip to decode for multi-clip mixing.
#[derive(Clone, Debug)]
struct AudioClipInfo {
    path: PathBuf,
    time: f64,
}

/// Request sent from UI to the audio decode thread.
enum AudioDecodeRequest {
    /// Decode and mix multiple audio clips simultaneously.
    SeekMulti {
        clips: Vec<AudioClipInfo>,
        continuous: bool,
    },
    Stop,
}

/// Decoded audio sent from the audio decode thread to the UI.
pub(crate) struct DecodedAudio {
    pub(crate) samples: Vec<f32>,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) pts_secs: f64,
}

/// Decoded frame sent from the decode thread to the UI.
pub(crate) struct DecodedFrame {
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Source-file PTS in seconds.
    pub(crate) pts_secs: f64,
}

pub struct App {
    pub project: Project,
    pub project_path: Option<PathBuf>,
    pub playback_position: TimelinePosition,
    pub is_playing: bool,
    pub status_message: String,
    pub selected_asset_id: Option<Uuid>,
    pub current_frame: Option<iced::widget::image::Handle>,
    pub playback_start_wall: Option<Instant>,
    pub playback_start_pos: TimelinePosition,
    pub timeline_zoom: f32,
    pub timeline_scroll: f32,
    pub tool_mode: ToolMode,
    pub open_menu: Option<MenuId>,
    pub thumbnails: HashMap<Uuid, iced::widget::image::Handle>,
    pub drag_state: Option<DragState>,
    pub hovered_asset_id: Option<Uuid>,
    pub selected_clip: Option<(usize, Uuid)>,
    pub confirm_dialog: Option<ConfirmDialog>,
    pub left_panel_tab: LeftPanelTab,
    pub track_context_menu: Option<TrackContextMenu>,
    /// Text input state for effect parameters with wide ranges (e.g. transform offset).
    /// Key: (effect_id, param_name), Value: current text string in the input field.
    pub effect_param_texts: HashMap<(Uuid, String), String>,
    decode_tx: Option<mpsc::Sender<DecodeRequest>>,
    pub(crate) decode_rx: Option<mpsc::Receiver<DecodedFrame>>,
    pub(crate) decode_clip_id: Option<Uuid>,
    /// IDs of all video clips currently being decoded (for multi-track change detection).
    decode_clip_ids: Vec<Uuid>,
    /// Offset to convert source PTS → timeline time: timeline_time = pts + offset.
    pub(crate) decode_time_offset: f64,
    /// Frame received from decode thread but not yet displayed (PTS ahead of playback).
    pending_frame: Option<DecodedFrame>,
    /// After a decode transition, discard frames that are too far ahead (stale from old context).
    drain_stale: bool,
    // Audio playback
    audio_player: Option<AudioPlayer>,
    audio_decode_tx: Option<mpsc::Sender<AudioDecodeRequest>>,
    pub(crate) audio_decode_rx: Option<mpsc::Receiver<DecodedAudio>>,
    /// ID of the audio clip currently being decoded (for change detection).
    pub(crate) audio_decode_clip_id: Option<Uuid>,
    pub(crate) audio_decode_time_offset: f64,
    // Render progress state
    pub is_rendering: bool,
    render_progress_rx: Option<mpsc::Receiver<zeditor_media::render_profile::RenderProgress>>,
    pub render_current_frame: u64,
    pub render_total_frames: u64,
    pub render_elapsed: Duration,
    pub render_start: Option<Instant>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            project: Project::new("Untitled"),
            project_path: None,
            playback_position: TimelinePosition::zero(),
            is_playing: false,
            status_message: String::new(),
            selected_asset_id: None,
            current_frame: None,
            playback_start_wall: None,
            playback_start_pos: TimelinePosition::zero(),
            timeline_zoom: 100.0,
            timeline_scroll: 0.0,
            tool_mode: ToolMode::default(),
            open_menu: None,
            thumbnails: HashMap::new(),
            drag_state: None,
            hovered_asset_id: None,
            selected_clip: None,
            confirm_dialog: None,
            left_panel_tab: LeftPanelTab::default(),
            track_context_menu: None,
            effect_param_texts: HashMap::new(),
            decode_tx: None,
            decode_rx: None,
            decode_clip_id: None,
            decode_clip_ids: Vec::new(),
            decode_time_offset: 0.0,
            pending_frame: None,
            drain_stale: false,
            audio_player: None,
            audio_decode_tx: None,
            audio_decode_rx: None,
            audio_decode_clip_id: None,
            audio_decode_time_offset: 0.0,
            is_rendering: false,
            render_progress_rx: None,
            render_current_frame: 0,
            render_total_frames: 0,
            render_elapsed: Duration::ZERO,
            render_start: None,
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(&self) -> String {
        format!("{} - Zeditor", self.project.name)
    }

    /// Reset all transient UI state (playback, decode, drag, thumbnails, etc.)
    /// Called after loading a project or creating a new one.
    fn reset_ui_state(&mut self) {
        self.playback_position = TimelinePosition::zero();
        self.is_playing = false;
        self.playback_start_wall = None;
        self.playback_start_pos = TimelinePosition::zero();
        self.current_frame = None;
        self.selected_asset_id = None;
        self.hovered_asset_id = None;
        self.selected_clip = None;
        self.confirm_dialog = None;
        self.left_panel_tab = LeftPanelTab::default();
        self.track_context_menu = None;
        self.thumbnails.clear();
        self.drag_state = None;
        self.timeline_zoom = 100.0;
        self.timeline_scroll = 0.0;
        self.tool_mode = ToolMode::default();
        self.open_menu = None;
        self.decode_clip_id = None;
        self.decode_clip_ids.clear();
        self.decode_time_offset = 0.0;
        self.pending_frame = None;
        self.drain_stale = false;
        self.audio_decode_clip_id = None;
        self.audio_decode_time_offset = 0.0;
        self.send_decode_stop();
        self.send_audio_decode_stop();
        if let Some(player) = &self.audio_player {
            player.stop();
        }
    }

    /// Generate thumbnail tasks for all assets in the source library.
    fn regenerate_all_thumbnails(&self) -> Task<Message> {
        let tasks: Vec<Task<Message>> = self
            .project
            .source_library
            .assets()
            .iter()
            .map(|asset| {
                let asset_id = asset.id;
                let path = asset.path.clone();
                Task::perform(
                    async move {
                        let result =
                            zeditor_media::thumbnail::generate_thumbnail_rgba_scaled(
                                &path, 160, 160,
                            )
                            .map(|frame| (frame.data, frame.width, frame.height))
                            .map_err(|e| format!("{e}"));
                        (asset_id, result)
                    },
                    |(asset_id, result)| Message::ThumbnailGenerated { asset_id, result },
                )
            })
            .collect();
        Task::batch(tasks)
    }

    pub fn boot() -> (Self, Task<Message>) {
        let (req_tx, req_rx) = mpsc::channel::<DecodeRequest>();
        let (frame_tx, frame_rx) = mpsc::sync_channel::<DecodedFrame>(1);

        std::thread::spawn(move || {
            decode_worker(req_rx, frame_tx);
        });

        // Audio decode thread
        let (audio_req_tx, audio_req_rx) = mpsc::channel::<AudioDecodeRequest>();
        let (audio_frame_tx, audio_frame_rx) = mpsc::sync_channel::<DecodedAudio>(16);

        std::thread::spawn(move || {
            audio_decode_worker(audio_req_rx, audio_frame_tx);
        });

        let mut app = Self::default();
        app.decode_tx = Some(req_tx);
        app.decode_rx = Some(frame_rx);
        app.audio_decode_tx = Some(audio_req_tx);
        app.audio_decode_rx = Some(audio_frame_rx);
        app.audio_player = AudioPlayer::new();
        (app, Task::none())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs: Vec<Subscription<Message>> =
            vec![keyboard::listen().map(Message::KeyboardEvent)];

        // Always tick: 16ms when playing (60fps), 100ms when paused (for scrub frames)
        let tick_ms = if self.is_playing { 16 } else { 100 };
        subs.push(time::every(Duration::from_millis(tick_ms)).map(|_| Message::PlaybackTick));

        // Global mouse tracking during drag
        if self.drag_state.is_some() {
            subs.push(event::listen_with(drag_event_filter));
        }

        Subscription::batch(subs)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenFileDialog => {
                self.status_message = "Opening file dialog...".into();
                Task::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("Video", &["mp4", "mov", "avi", "mkv", "webm"])
                            .set_title("Import Media")
                            .pick_files()
                            .await;
                        match handle {
                            Some(files) => {
                                files.into_iter().map(|f| f.path().to_path_buf()).collect()
                            }
                            None => Vec::new(),
                        }
                    },
                    Message::FileDialogResult,
                )
            }
            Message::FileDialogResult(paths) => {
                if paths.is_empty() {
                    self.status_message = "Import cancelled".into();
                    return Task::none();
                }
                let tasks: Vec<Task<Message>> = paths
                    .into_iter()
                    .map(|path| {
                        Task::perform(
                            async move {
                                zeditor_media::probe::probe(&path)
                                    .map_err(|e| format!("{e}"))
                            },
                            Message::MediaImported,
                        )
                    })
                    .collect();
                Task::batch(tasks)
            }
            Message::ImportMedia(path) => {
                self.status_message = "Importing...".into();
                Task::perform(
                    async move {
                        zeditor_media::probe::probe(&path).map_err(|e| format!("{e}"))
                    },
                    Message::MediaImported,
                )
            }
            Message::MediaImported(result) => {
                match result {
                    Ok(asset) => {
                        self.status_message = format!("Imported: {}", asset.name);
                        let asset_id = asset.id;
                        let path = asset.path.clone();
                        self.project.source_library.import(asset);
                        // Spawn thumbnail generation in the background
                        return Task::perform(
                            async move {
                                let result = zeditor_media::thumbnail::generate_thumbnail_rgba_scaled(
                                    &path, 160, 160,
                                )
                                .map(|frame| (frame.data, frame.width, frame.height))
                                .map_err(|e| format!("{e}"));
                                (asset_id, result)
                            },
                            |(asset_id, result)| Message::ThumbnailGenerated { asset_id, result },
                        );
                    }
                    Err(e) => {
                        self.status_message = format!("Import failed: {e}");
                    }
                }
                Task::none()
            }
            Message::ThumbnailGenerated { asset_id, result } => {
                if let Ok((data, width, height)) = result {
                    let handle = iced::widget::image::Handle::from_rgba(width, height, data);
                    self.thumbnails.insert(asset_id, handle);
                }
                Task::none()
            }
            Message::RemoveAsset(id) => {
                // Remove all clips using this asset (via command history for undo)
                let clips_using = self.project.timeline.clips_using_asset(id);
                if !clips_using.is_empty() {
                    self.project.command_history.execute(
                        &mut self.project.timeline,
                        "Remove clips for asset",
                        |tl| { tl.remove_clips_by_asset(id); Ok(()) },
                    ).ok();
                }
                match self.project.source_library.remove(id) {
                    Ok(asset) => {
                        self.status_message = format!("Removed: {}", asset.name);
                        self.thumbnails.remove(&id);
                        if self.selected_asset_id == Some(id) {
                            self.selected_asset_id = None;
                        }
                    }
                    Err(e) => {
                        self.status_message = format!("Remove failed: {e}");
                    }
                }
                Task::none()
            }
            Message::ConfirmRemoveAsset(asset_id) => {
                let clips_using = self.project.timeline.clips_using_asset(asset_id);
                if clips_using.is_empty() {
                    // No clips in use — remove directly
                    return self.update(Message::RemoveAsset(asset_id));
                }
                // Clips in use — show confirmation dialog
                let count = clips_using.len();
                self.confirm_dialog = Some(ConfirmDialog {
                    message: format!(
                        "This asset is used by {count} clip(s) in the timeline. Delete the asset and all its clips?"
                    ),
                    action: ConfirmAction::RemoveAsset { asset_id },
                });
                Task::none()
            }
            Message::ConfirmDialogAccepted => {
                if let Some(dialog) = self.confirm_dialog.take() {
                    match dialog.action {
                        ConfirmAction::RemoveAsset { asset_id } => {
                            return self.update(Message::RemoveAsset(asset_id));
                        }
                    }
                }
                Task::none()
            }
            Message::ConfirmDialogDismissed => {
                self.confirm_dialog = None;
                Task::none()
            }
            Message::SelectTimelineClip(selection) => {
                self.selected_clip = selection;
                Task::none()
            }
            Message::RemoveClip {
                track_index,
                clip_id,
            } => {
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Remove clip",
                    |tl| tl.remove_clip_grouped(track_index, clip_id),
                );
                match result {
                    Ok(()) => {
                        self.status_message = "Clip removed".into();
                        self.selected_clip = None;
                    }
                    Err(e) => {
                        self.status_message = format!("Remove clip failed: {e}");
                    }
                }
                Task::none()
            }
            Message::SelectSourceAsset(id) => {
                self.selected_asset_id = id;
                Task::none()
            }
            Message::SourceCardHovered(id) => {
                self.hovered_asset_id = id;
                Task::none()
            }
            Message::StartDragFromSource(asset_id) => {
                if let Some(asset) = self.project.source_library.get(asset_id) {
                    let thumbnail = self.thumbnails.get(&asset_id).cloned();
                    self.drag_state = Some(DragState {
                        payload: DragPayload::SourceAsset {
                            asset_id,
                            thumbnail,
                            name: asset.name.clone(),
                        },
                        cursor_position: Point::ORIGIN,
                        over_timeline: false,
                        timeline_track: None,
                        timeline_position: None,
                    });
                }
                Task::none()
            }
            Message::DragMoved(position) => {
                if let Some(drag) = &mut self.drag_state {
                    drag.cursor_position = position;
                }
                Task::none()
            }
            Message::DragReleased => {
                if let Some(drag) = self.drag_state.take() {
                    if drag.over_timeline {
                        let DragPayload::SourceAsset { asset_id, .. } = &drag.payload;
                        if let (Some(track_index), Some(position)) =
                            (drag.timeline_track, drag.timeline_position)
                        {
                            return self.update(Message::AddClipToTimeline {
                                asset_id: *asset_id,
                                track_index,
                                position,
                            });
                        }
                    }
                }
                Task::none()
            }
            Message::DragEnteredTimeline => {
                if let Some(drag) = &mut self.drag_state {
                    drag.over_timeline = true;
                }
                Task::none()
            }
            Message::DragExitedTimeline => {
                if let Some(drag) = &mut self.drag_state {
                    drag.over_timeline = false;
                    drag.timeline_track = None;
                    drag.timeline_position = None;
                }
                Task::none()
            }
            Message::DragOverTimeline(point) => {
                if let Some(drag) = &mut self.drag_state {
                    // Account for controls row height (~30px), header column, and ruler height (20px)
                    let controls_height = 30.0_f32;
                    let ruler_height = 20.0_f32;
                    let track_height = 50.0_f32;
                    let header_width = 60.0_f32;

                    let canvas_x = (point.x - header_width).max(0.0);
                    let secs = ((canvas_x + self.timeline_scroll) / self.timeline_zoom) as f64;
                    let secs = secs.max(0.0);

                    let track_y = point.y - controls_height - ruler_height;
                    let track_index = if track_y < 0.0 {
                        0
                    } else {
                        let idx = (track_y / track_height) as usize;
                        idx.min(self.project.timeline.tracks.len().saturating_sub(1))
                    };

                    // Snap to nearest video track (for assets with video)
                    let video_track_index = self
                        .project
                        .timeline
                        .tracks
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| t.track_type == TrackType::Video)
                        .map(|(i, _)| i)
                        .min_by_key(|&i| {
                            let diff = if i > track_index { i - track_index } else { track_index - i };
                            diff
                        })
                        .unwrap_or(0);

                    drag.timeline_track = Some(video_track_index);
                    drag.timeline_position = Some(TimelinePosition::from_secs_f64(secs));
                }
                Task::none()
            }
            Message::AddClipToTimeline {
                asset_id,
                track_index,
                position,
            } => {
                if let Some(asset) = self.project.source_library.get(asset_id) {
                    let source_range = TimeRange {
                        start: TimelinePosition::zero(),
                        end: TimelinePosition::from_secs_f64(asset.duration.as_secs_f64()),
                    };
                    let has_audio = asset.has_audio;
                    let audio_track = self.project.timeline.find_paired_audio_track(track_index);

                    if has_audio && audio_track.is_some() {
                        let audio_track = audio_track.unwrap();
                        let result = self.project.command_history.execute(
                            &mut self.project.timeline,
                            "Add clip",
                            |tl| {
                                tl.add_clip_with_audio(track_index, audio_track, asset_id, position, source_range)
                            },
                        );
                        match result {
                            Ok(_) => {
                                self.status_message = "Clip added".into();
                            }
                            Err(e) => {
                                self.status_message = format!("Add clip failed: {e}");
                            }
                        }
                    } else {
                        let clip = Clip::new(asset_id, position, source_range);
                        let result = self.project.command_history.execute(
                            &mut self.project.timeline,
                            "Add clip",
                            |tl| tl.add_clip_trimming_overlaps(track_index, clip),
                        );
                        match result {
                            Ok(()) => {
                                self.status_message = "Clip added".into();
                            }
                            Err(e) => {
                                self.status_message = format!("Add clip failed: {e}");
                            }
                        }
                    }
                } else {
                    self.status_message = "Asset not found".into();
                }
                Task::none()
            }
            Message::MoveClip {
                source_track,
                clip_id,
                dest_track,
                position,
            } => {
                let snap_threshold = Duration::from_millis(200);
                let has_link = self.project.timeline.track(source_track)
                    .ok()
                    .and_then(|t| t.get_clip(clip_id))
                    .and_then(|c| c.link_id)
                    .is_some();

                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Move clip",
                    |tl| {
                        if has_link {
                            tl.move_clip_grouped(source_track, clip_id, dest_track, position)?;
                        } else {
                            tl.move_clip(source_track, clip_id, dest_track, position)?;
                        }
                        let _ = tl.snap_to_adjacent(dest_track, clip_id, snap_threshold);
                        Ok(())
                    },
                );
                if let Err(e) = result {
                    self.status_message = format!("Move failed: {e}");
                }
                Task::none()
            }
            Message::CutClip {
                track_index,
                position,
            } => {
                let has_link = self.project.timeline.track(track_index)
                    .ok()
                    .and_then(|t| t.clip_at(position))
                    .and_then(|c| c.link_id)
                    .is_some();

                let result = if has_link {
                    self.project.command_history.execute(
                        &mut self.project.timeline,
                        "Cut clip",
                        |tl| tl.cut_at_grouped(track_index, position),
                    )
                } else {
                    self.project.command_history.execute(
                        &mut self.project.timeline,
                        "Cut clip",
                        |tl| tl.cut_at(track_index, position).map(|pair| vec![pair]),
                    )
                };
                match result {
                    Ok(_) => {
                        self.status_message = "Clip cut".into();
                    }
                    Err(e) => {
                        self.status_message = format!("Cut failed: {e}");
                    }
                }
                Task::none()
            }
            Message::ResizeClip {
                track_index,
                clip_id,
                new_end,
            } => {
                let has_link = self.project.timeline.track(track_index)
                    .ok()
                    .and_then(|t| t.get_clip(clip_id))
                    .and_then(|c| c.link_id)
                    .is_some();

                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Resize clip",
                    |tl| {
                        if has_link {
                            tl.resize_clip_grouped(track_index, clip_id, new_end)
                        } else {
                            tl.resize_clip(track_index, clip_id, new_end)
                        }
                    },
                );
                if let Err(e) = result {
                    self.status_message = format!("Resize failed: {e}");
                }
                Task::none()
            }
            Message::TimelineZoom { delta, cursor_secs } => {
                // Zoom centered on cursor position
                let old_zoom = self.timeline_zoom;
                let factor = 1.0 + delta * 0.15;
                self.timeline_zoom =
                    (self.timeline_zoom * factor).clamp(0.1, 1000.0);

                // Adjust scroll so the point under the cursor stays put
                let cursor_px_before = cursor_secs as f32 * old_zoom - self.timeline_scroll;
                let cursor_px_after = cursor_secs as f32 * self.timeline_zoom - self.timeline_scroll;
                self.timeline_scroll += cursor_px_after - cursor_px_before;
                self.timeline_scroll = self.timeline_scroll.max(0.0);
                Task::none()
            }
            Message::TimelineScroll(delta_px) => {
                self.timeline_scroll = (self.timeline_scroll + delta_px).max(0.0);
                Task::none()
            }
            Message::TimelineClickEmpty(pos) => {
                self.selected_clip = None;
                if self.is_playing {
                    self.is_playing = false;
                    self.playback_start_wall = None;
                    self.send_audio_decode_stop();
                    if let Some(player) = &self.audio_player {
                        player.pause();
                    }
                }
                self.playback_position = pos;
                self.send_decode_seek(false); // scrub, not continuous
                Task::none()
            }
            Message::Play => {
                self.is_playing = true;
                self.playback_start_wall = Some(Instant::now());
                self.playback_start_pos = self.playback_position;
                self.send_decode_seek(true);
                self.send_audio_decode_seek(true);
                if let Some(player) = &self.audio_player {
                    player.play();
                }
                Task::none()
            }
            Message::Pause => {
                self.is_playing = false;
                self.playback_start_wall = None;
                self.send_decode_stop();
                self.send_audio_decode_stop();
                if let Some(player) = &self.audio_player {
                    player.pause();
                }
                Task::none()
            }
            Message::TogglePlayback => {
                if self.is_playing {
                    self.update(Message::Pause)
                } else {
                    self.update(Message::Play)
                }
            }
            Message::PlaybackTick => {
                if let Some(start_wall) = self.playback_start_wall {
                    let elapsed = start_wall.elapsed().as_secs_f64();
                    let new_pos = self.playback_start_pos.as_secs_f64() + elapsed;
                    self.playback_position = TimelinePosition::from_secs_f64(new_pos);

                    // Auto-scroll: keep playhead visible
                    let playhead_px =
                        new_pos as f32 * self.timeline_zoom - self.timeline_scroll;
                    let visible_width = 800.0; // approximate
                    if playhead_px > visible_width * 0.8 {
                        self.timeline_scroll =
                            new_pos as f32 * self.timeline_zoom - visible_width * 0.5;
                    }

                    // Check if we've crossed into a different set of video clips
                    let current_clip_ids: Vec<Uuid> =
                        self.all_video_clips_at_position(self.playback_position)
                            .iter().map(|(_, c)| c.id).collect();
                    if current_clip_ids != self.decode_clip_ids {
                        self.send_decode_seek(true);
                        self.drain_stale = true;
                    }

                    // Check if the primary audio clip changed
                    let current_audio_id = self.audio_clip_at_position(self.playback_position)
                        .map(|(_, c)| c.id);
                    if current_audio_id != self.audio_decode_clip_id {
                        self.send_audio_decode_seek(true);
                    }
                }

                // Drain decoded frames from the channels
                self.poll_decoded_frame();
                self.poll_decoded_audio();

                // Poll render progress if rendering
                if self.is_rendering {
                    if let Some(ref rx) = self.render_progress_rx {
                        while let Ok(progress) = rx.try_recv() {
                            self.render_current_frame = progress.current_frame;
                            self.render_total_frames = progress.total_frames;
                            self.render_elapsed = progress.elapsed;
                        }
                        if self.render_total_frames > 0 {
                            let pct = self.render_current_frame as f64
                                / self.render_total_frames as f64
                                * 100.0;
                            let elapsed = self.render_elapsed.as_secs();
                            let mins = elapsed / 60;
                            let secs = elapsed % 60;
                            self.status_message = format!(
                                "Rendering: {}/{} frames ({:.1}%) | Elapsed: {}:{:02}",
                                self.render_current_frame,
                                self.render_total_frames,
                                pct,
                                mins,
                                secs,
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::SeekTo(pos) => {
                self.playback_position = pos;
                if self.is_playing {
                    self.playback_start_wall = Some(Instant::now());
                    self.playback_start_pos = pos;
                }
                self.send_decode_seek(self.is_playing);
                Task::none()
            }
            Message::KeyboardEvent(event) => {
                if let keyboard::Event::KeyPressed { key, .. } = event {
                    // Escape cancels drag
                    if self.drag_state.is_some() {
                        if matches!(key.as_ref(), keyboard::Key::Named(keyboard::key::Named::Escape)) {
                            self.drag_state = None;
                        }
                        return Task::none();
                    }
                    // When a track context menu is open, Escape dismisses it and all other keys are swallowed
                    if self.track_context_menu.is_some() {
                        if matches!(key.as_ref(), keyboard::Key::Named(keyboard::key::Named::Escape)) {
                            self.track_context_menu = None;
                        }
                        return Task::none();
                    }
                    // When a confirm dialog is open, Escape dismisses it and all other keys are swallowed
                    if self.confirm_dialog.is_some() {
                        if matches!(key.as_ref(), keyboard::Key::Named(keyboard::key::Named::Escape)) {
                            self.confirm_dialog = None;
                        }
                        return Task::none();
                    }
                    // When a menu is open, Escape closes it and all other keys are swallowed
                    if self.open_menu.is_some() {
                        if matches!(key.as_ref(), keyboard::Key::Named(keyboard::key::Named::Escape)) {
                            self.open_menu = None;
                        }
                        return Task::none();
                    }
                    match key.as_ref() {
                        keyboard::Key::Named(keyboard::key::Named::Space) => {
                            return self.update(Message::TogglePlayback);
                        }
                        keyboard::Key::Named(keyboard::key::Named::Delete) | keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                            if let Some((track_index, clip_id)) = self.selected_clip {
                                return self.update(Message::RemoveClip {
                                    track_index,
                                    clip_id,
                                });
                            }
                            if let Some(asset_id) = self.selected_asset_id {
                                return self.update(Message::ConfirmRemoveAsset(asset_id));
                            }
                        }
                        keyboard::Key::Character("a") => {
                            self.tool_mode = ToolMode::Arrow;
                        }
                        keyboard::Key::Character("b") => {
                            self.tool_mode = ToolMode::Blade;
                        }
                        _ => {}
                    }
                }
                Task::none()
            }
            Message::Undo => {
                match self
                    .project
                    .command_history
                    .undo(&mut self.project.timeline)
                {
                    Ok(()) => {
                        self.status_message = "Undone".into();
                    }
                    Err(e) => {
                        self.status_message = format!("Undo failed: {e}");
                    }
                }
                Task::none()
            }
            Message::Redo => {
                match self
                    .project
                    .command_history
                    .redo(&mut self.project.timeline)
                {
                    Ok(()) => {
                        self.status_message = "Redone".into();
                    }
                    Err(e) => {
                        self.status_message = format!("Redo failed: {e}");
                    }
                }
                Task::none()
            }
            Message::SaveProject => {
                if let Some(path) = &self.project_path {
                    // Save directly to the known path
                    match self.project.save(path) {
                        Ok(()) => {
                            self.status_message = format!("Saved to {}", path.display());
                        }
                        Err(e) => {
                            self.status_message = format!("Save failed: {e}");
                        }
                    }
                    Task::none()
                } else {
                    // No path yet — open save dialog
                    self.status_message = "Opening save dialog...".into();
                    Task::perform(
                        async {
                            let handle = rfd::AsyncFileDialog::new()
                                .add_filter("Zeditor Project", &["zpf"])
                                .set_title("Save Project")
                                .save_file()
                                .await;
                            handle.map(|f| f.path().to_path_buf())
                        },
                        Message::SaveFileDialogResult,
                    )
                }
            }
            Message::SaveFileDialogResult(path) => {
                match path {
                    Some(mut path) => {
                        // Ensure .zpf extension
                        if path.extension().is_none_or(|e| e != "zpf") {
                            path.set_extension("zpf");
                        }
                        // Derive project name from filename stem
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            self.project.name = stem.to_string();
                        }
                        match self.project.save(&path) {
                            Ok(()) => {
                                self.status_message = format!("Saved to {}", path.display());
                                self.project_path = Some(path);
                            }
                            Err(e) => {
                                self.status_message = format!("Save failed: {e}");
                            }
                        }
                    }
                    None => {
                        self.status_message = "Save cancelled".into();
                    }
                }
                Task::none()
            }
            Message::LoadProject(path) => {
                match zeditor_core::project::Project::load(&path) {
                    Ok(project) => {
                        self.reset_ui_state();
                        self.project = project;
                        self.project_path = Some(path.clone());
                        self.status_message = format!("Loaded {}", path.display());
                        self.regenerate_all_thumbnails()
                    }
                    Err(e) => {
                        self.status_message = format!("Load failed: {e}");
                        Task::none()
                    }
                }
            }
            Message::LoadFileDialogResult(path) => {
                match path {
                    Some(path) => {
                        return self.update(Message::LoadProject(path));
                    }
                    None => {
                        self.status_message = "Load cancelled".into();
                    }
                }
                Task::none()
            }
            Message::NewProject => {
                self.reset_ui_state();
                self.project = Project::default();
                self.project_path = None;
                self.status_message = "New project created".into();
                Task::none()
            }
            Message::RenderFileDialogResult(path) => {
                match path {
                    Some(mut path) => {
                        // Ensure .mkv extension
                        if path.extension().is_none_or(|e| e != "mkv") {
                            path.set_extension("mkv");
                        }
                        self.status_message = "Rendering...".into();
                        let timeline = self.project.timeline.clone();
                        let source_library = self.project.source_library.clone();
                        let config = zeditor_media::renderer::derive_render_config(
                            &timeline,
                            &source_library,
                            &self.project.settings,
                            path,
                        );
                        // Create a progress channel for render progress updates
                        let (ptx, prx) = std::sync::mpsc::channel();
                        self.is_rendering = true;
                        self.render_start = Some(Instant::now());
                        self.render_current_frame = 0;
                        self.render_total_frames = 0;
                        self.render_elapsed = Duration::ZERO;
                        self.render_progress_rx = Some(prx);

                        Task::perform(
                            async move {
                                zeditor_media::renderer::render_timeline(
                                    &timeline,
                                    &source_library,
                                    &config,
                                    Some(ptx),
                                )
                                .map(|()| config.output_path.clone())
                                .map_err(|e| format!("{e}"))
                            },
                            |result| match result {
                                Ok(path) => Message::RenderComplete(path),
                                Err(e) => Message::RenderError(e),
                            },
                        )
                    }
                    None => {
                        self.status_message = "Render cancelled".into();
                        Task::none()
                    }
                }
            }
            Message::RenderComplete(path) => {
                let total_time = self
                    .render_start
                    .map(|s| s.elapsed())
                    .unwrap_or_default();
                let total_secs = total_time.as_secs();
                let total_ms = total_time.subsec_millis();
                self.status_message = format!(
                    "Rendered to {} | Total time: {}:{:02}.{:03}",
                    path.display(),
                    total_secs / 60,
                    total_secs % 60,
                    total_ms,
                );
                self.is_rendering = false;
                self.render_progress_rx = None;
                self.render_start = None;
                Task::none()
            }
            Message::RenderError(msg) => {
                self.status_message = format!("Render failed: {msg}");
                self.is_rendering = false;
                self.render_progress_rx = None;
                self.render_start = None;
                Task::none()
            }
            Message::MenuButtonClicked(id) => {
                if self.open_menu == Some(id) {
                    self.open_menu = None;
                } else {
                    self.open_menu = Some(id);
                }
                Task::none()
            }
            Message::MenuButtonHovered(id) => {
                if self.open_menu.is_some() {
                    self.open_menu = Some(id);
                }
                Task::none()
            }
            Message::CloseMenu => {
                self.open_menu = None;
                Task::none()
            }
            Message::MenuAction(action) => {
                self.open_menu = None;
                match action {
                    MenuAction::Undo => self.update(Message::Undo),
                    MenuAction::Redo => self.update(Message::Redo),
                    MenuAction::Exit => self.update(Message::Exit),
                    MenuAction::NewProject => self.update(Message::NewProject),
                    MenuAction::LoadProject => {
                        self.status_message = "Opening load dialog...".into();
                        Task::perform(
                            async {
                                let handle = rfd::AsyncFileDialog::new()
                                    .add_filter("Zeditor Project", &["zpf"])
                                    .set_title("Load Project")
                                    .pick_file()
                                    .await;
                                handle.map(|f| f.path().to_path_buf())
                            },
                            Message::LoadFileDialogResult,
                        )
                    }
                    MenuAction::Save => self.update(Message::SaveProject),
                    MenuAction::Render => {
                        self.status_message = "Opening render dialog...".into();
                        Task::perform(
                            async {
                                let handle = rfd::AsyncFileDialog::new()
                                    .add_filter("MKV Video", &["mkv"])
                                    .set_title("Render Output")
                                    .save_file()
                                    .await;
                                handle.map(|f| f.path().to_path_buf())
                            },
                            Message::RenderFileDialogResult,
                        )
                    }
                }
            }
            Message::Exit => iced::exit(),
            Message::ShowTrackContextMenu { track_index, screen_position } => {
                if let Some(track) = self.project.timeline.tracks.get(track_index) {
                    self.track_context_menu = Some(TrackContextMenu {
                        track_index,
                        position: screen_position,
                        track_type: track.track_type,
                    });
                }
                Task::none()
            }
            Message::DismissTrackContextMenu => {
                self.track_context_menu = None;
                Task::none()
            }
            Message::AddVideoTrackAbove(ref_idx) => {
                self.track_context_menu = None;
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Add video track above",
                    |tl| { tl.insert_video_track_above(ref_idx)?; Ok(()) },
                );
                match result {
                    Ok(()) => {
                        self.status_message = "Video track added".into();
                        // Clear selected clip since indices may have shifted
                        self.selected_clip = None;
                    }
                    Err(e) => self.status_message = format!("Add track failed: {e}"),
                }
                Task::none()
            }
            Message::AddVideoTrackBelow(ref_idx) => {
                self.track_context_menu = None;
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Add video track below",
                    |tl| { tl.insert_video_track_below(ref_idx)?; Ok(()) },
                );
                match result {
                    Ok(()) => {
                        self.status_message = "Video track added".into();
                        self.selected_clip = None;
                    }
                    Err(e) => self.status_message = format!("Add track failed: {e}"),
                }
                Task::none()
            }
            Message::AddAudioTrackAbove(ref_idx) => {
                self.track_context_menu = None;
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Add audio track above",
                    |tl| { tl.insert_audio_track_above(ref_idx)?; Ok(()) },
                );
                match result {
                    Ok(()) => {
                        self.status_message = "Audio track added".into();
                        self.selected_clip = None;
                    }
                    Err(e) => self.status_message = format!("Add track failed: {e}"),
                }
                Task::none()
            }
            Message::AddAudioTrackBelow(ref_idx) => {
                self.track_context_menu = None;
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Add audio track below",
                    |tl| { tl.insert_audio_track_below(ref_idx)?; Ok(()) },
                );
                match result {
                    Ok(()) => {
                        self.status_message = "Audio track added".into();
                        self.selected_clip = None;
                    }
                    Err(e) => self.status_message = format!("Add track failed: {e}"),
                }
                Task::none()
            }
            Message::SwitchLeftPanelTab(tab) => {
                self.left_panel_tab = tab;
                Task::none()
            }
            Message::AddEffectToSelectedClip(effect_type) => {
                if let Some((track_index, clip_id)) = self.selected_clip {
                    let result = self.project.command_history.execute(
                        &mut self.project.timeline,
                        "Add effect",
                        |tl| {
                            let clip = tl.track_mut(track_index)?
                                .get_clip_mut(clip_id)
                                .ok_or(zeditor_core::error::CoreError::ClipNotFound(clip_id))?;
                            clip.effects.push(EffectInstance::new(effect_type));
                            Ok(())
                        },
                    );
                    match result {
                        Ok(()) => self.status_message = "Effect added".into(),
                        Err(e) => self.status_message = format!("Add effect failed: {e}"),
                    }
                }
                Task::none()
            }
            Message::RemoveEffectFromClip { track_index, clip_id, effect_id } => {
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Remove effect",
                    |tl| {
                        let clip = tl.track_mut(track_index)?
                            .get_clip_mut(clip_id)
                            .ok_or(zeditor_core::error::CoreError::ClipNotFound(clip_id))?;
                        clip.effects.retain(|e| e.id != effect_id);
                        Ok(())
                    },
                );
                match result {
                    Ok(()) => self.status_message = "Effect removed".into(),
                    Err(e) => self.status_message = format!("Remove effect failed: {e}"),
                }
                Task::none()
            }
            Message::UpdateEffectParameter { track_index, clip_id, effect_id, param_name, value } => {
                // Clear text input state when slider updates the value
                self.effect_param_texts.remove(&(effect_id, param_name.clone()));
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Update effect parameter",
                    |tl| {
                        let clip = tl.track_mut(track_index)?
                            .get_clip_mut(clip_id)
                            .ok_or(zeditor_core::error::CoreError::ClipNotFound(clip_id))?;
                        if let Some(effect) = clip.effects.iter_mut().find(|e| e.id == effect_id) {
                            effect.set_float(&param_name, value);
                        }
                        Ok(())
                    },
                );
                if let Err(e) = result {
                    self.status_message = format!("Update effect failed: {e}");
                }
                // Trigger decode refresh to update preview
                self.send_decode_seek(false);
                Task::none()
            }
            Message::EffectParamTextInput { track_index, clip_id, effect_id, param_name, text: input_text } => {
                // Store the raw text for display
                self.effect_param_texts.insert((effect_id, param_name.clone()), input_text.clone());

                // If parseable as a number, also update the effect parameter
                if let Ok(value) = input_text.parse::<f64>() {
                    // Look up min/max bounds from parameter definitions
                    let in_bounds = self.project.timeline
                        .track(track_index)
                        .ok()
                        .and_then(|t| t.get_clip(clip_id))
                        .and_then(|c| c.effects.iter().find(|e| e.id == effect_id))
                        .map(|effect| {
                            for def in effect.effect_type.parameter_definitions() {
                                if def.name == param_name {
                                    let zeditor_core::effects::ParameterType::Float { min, max, .. } = def.param_type;
                                    return value >= min && value <= max;
                                }
                            }
                            false
                        })
                        .unwrap_or(false);

                    if in_bounds {
                        let result = self.project.command_history.execute(
                            &mut self.project.timeline,
                            "Update effect parameter",
                            |tl| {
                                let clip = tl.track_mut(track_index)?
                                    .get_clip_mut(clip_id)
                                    .ok_or(zeditor_core::error::CoreError::ClipNotFound(clip_id))?;
                                if let Some(effect) = clip.effects.iter_mut().find(|e| e.id == effect_id) {
                                    effect.set_float(&param_name, value);
                                }
                                Ok(())
                            },
                        );
                        if let Err(e) = result {
                            self.status_message = format!("Update effect failed: {e}");
                        }
                        self.send_decode_seek(false);
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let menu_bar = self.view_menu_bar();
        let source_panel = self.view_left_panel();
        let video_viewport = self.view_video_viewport();
        let timeline_panel = self.view_timeline();
        let effects_inspector = self.view_clip_effects_inspector();

        let pos_secs = self.playback_position.as_secs_f64();
        let total_secs = pos_secs as u64;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        let millis = ((pos_secs - total_secs as f64) * 1000.0) as u64;

        // Playback info line (above timeline)
        let playback_info = text(format!(
            "{:02}:{:02}:{:02}.{:03} | Zoom: {:.0}% | {}",
            hours,
            minutes,
            seconds,
            millis,
            self.timeline_zoom,
            if self.is_playing { "Playing" } else { "Stopped" }
        ))
        .size(14);

        // System message status bar (bottom of window)
        let status_bar: Element<'_, Message> = if self.is_rendering && self.render_total_frames > 0
        {
            let pct = self.render_current_frame as f64
                / self.render_total_frames as f64
                * 100.0;
            let elapsed = self.render_elapsed.as_secs();
            let mins = elapsed / 60;
            let secs = elapsed % 60;
            let progress_text = format!(
                "Rendering: {}/{} frames ({:.1}%) | Elapsed: {}:{:02}",
                self.render_current_frame, self.render_total_frames, pct, mins, secs,
            );

            let pct_frac = (pct / 100.0).min(1.0) as f32;

            let bar = container(
                row![
                    // Green progress fill
                    container(Space::new().height(4))
                        .width(Length::FillPortion((pct_frac * 1000.0) as u16))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color::from_rgb(
                                0.2, 0.8, 0.3,
                            ))),
                            ..Default::default()
                        }),
                    // Empty portion
                    container(Space::new().height(4))
                        .width(Length::FillPortion(
                            ((1.0 - pct_frac) * 1000.0) as u16,
                        ))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color::from_rgb(
                                0.3, 0.3, 0.33,
                            ))),
                            ..Default::default()
                        }),
                ]
                .width(Length::Fill),
            )
            .width(Length::Fill);

            container(
                column![
                    text(progress_text)
                        .size(13)
                        .color(Color::from_rgb(1.0, 0.9, 0.3)),
                    bar,
                ]
                .spacing(2),
            )
            .padding([4, 8])
            .width(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(0.18, 0.18, 0.20))),
                border: Border {
                    color: Color::from_rgb(0.15, 0.15, 0.17),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
        } else {
            let status_message = if self.status_message.is_empty() {
                "No system messages"
            } else {
                &self.status_message
            };
            container(
                text(status_message)
                    .size(13)
                    .color(Color::from_rgb(0.7, 0.7, 0.7)),
            )
            .padding([4, 8])
            .width(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(0.18, 0.18, 0.20))),
                border: Border {
                    color: Color::from_rgb(0.15, 0.15, 0.17),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
        };

        let top_row = row![source_panel, video_viewport].spacing(4);

        // Timeline row: timeline panel + effects inspector
        let timeline_row: Element<'_, Message> = row![timeline_panel, effects_inspector]
            .spacing(4)
            .height(Length::Fill)
            .into();

        // Main content area (without status bar)
        let main_content: Element<'_, Message> = if self.open_menu.is_some() {
            let click_off: Element<'_, Message> = mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_press(Message::CloseMenu)
            .into();

            let dropdown = self.view_dropdown();

            let content_below = column![top_row, timeline_row, playback_info].spacing(4);

            let stacked_content = stack![content_below, click_off, opaque(dropdown)]
                .width(Length::Fill)
                .height(Length::Fill);

            column![menu_bar, stacked_content]
                .spacing(4)
                .padding(4)
                .into()
        } else {
            column![menu_bar, top_row, timeline_row, playback_info]
                .spacing(4)
                .padding(4)
                .into()
        };

        // Wrap main content in a container that takes up remaining space
        let main_container = container(main_content)
            .width(Length::Fill)
            .height(Length::Fill);

        // Layout with status bar pinned to bottom
        let base_layout: Element<'_, Message> = column![main_container, status_bar]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        // Add confirmation dialog overlay if present
        let base_layout: Element<'_, Message> = if let Some(dialog) = &self.confirm_dialog {
            let click_off: Element<'_, Message> = mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_press(Message::ConfirmDialogDismissed)
            .into();

            let dialog_card = container(
                column![
                    text(&dialog.message).size(14).color(Color::WHITE),
                    row![
                        button(text("Delete").size(14).color(Color::WHITE))
                            .on_press(Message::ConfirmDialogAccepted)
                            .padding([6, 16])
                            .style(|_theme, _status| button::Style {
                                background: Some(Background::Color(Color::from_rgb(0.8, 0.2, 0.2))),
                                text_color: Color::WHITE,
                                border: Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                        button(text("Cancel").size(14).color(Color::WHITE))
                            .on_press(Message::ConfirmDialogDismissed)
                            .padding([6, 16])
                            .style(|_theme, _status| button::Style {
                                background: Some(Background::Color(Color::from_rgb(0.3, 0.3, 0.33))),
                                text_color: Color::WHITE,
                                border: Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                    ]
                    .spacing(8)
                ]
                .spacing(12),
            )
            .padding(20)
            .width(400)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(0.22, 0.22, 0.25))),
                border: Border {
                    color: Color::from_rgb(0.4, 0.4, 0.45),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            });

            let centered_dialog = center(dialog_card)
                .width(Length::Fill)
                .height(Length::Fill);

            stack![base_layout, click_off, opaque(centered_dialog)]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            base_layout
        };

        // Add drag ghost overlay if dragging
        let content: Element<'_, Message> = if let Some(drag) = &self.drag_state {
            let ghost = self.view_drag_overlay(drag);
            stack![base_layout, ghost]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            base_layout
        };

        // Wrap in background container (#2b2d31) for consistent dark background on all platforms
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(
                    0x2b as f32 / 255.0,
                    0x2d as f32 / 255.0,
                    0x31 as f32 / 255.0,
                ))),
                ..Default::default()
            })
            .into()
    }

    fn view_left_panel(&self) -> Element<'_, Message> {
        let tab_button = |label: &'static str, tab: LeftPanelTab| -> Element<'_, Message> {
            let is_active = self.left_panel_tab == tab;
            button(text(label).size(13).color(Color::WHITE))
                .on_press(Message::SwitchLeftPanelTab(tab))
                .padding([4, 10])
                .style(move |_theme, _status| {
                    let bg = if is_active {
                        Color::from_rgb(0.35, 0.35, 0.38)
                    } else {
                        Color::from_rgb(0.22, 0.22, 0.24)
                    };
                    button::Style {
                        background: Some(Background::Color(bg)),
                        text_color: Color::WHITE,
                        border: Border {
                            radius: 4.0.into(),
                            color: if is_active { Color::from_rgb(0.5, 0.5, 0.55) } else { Color::TRANSPARENT },
                            width: if is_active { 1.0 } else { 0.0 },
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .into()
        };

        let tabs = row![
            tab_button("Project Library", LeftPanelTab::ProjectLibrary),
            tab_button("Effects", LeftPanelTab::Effects),
        ]
        .spacing(4);

        let content: Element<'_, Message> = match self.left_panel_tab {
            LeftPanelTab::ProjectLibrary => self.view_source_library_content(),
            LeftPanelTab::Effects => self.view_effects_browser(),
        };

        column![tabs, content]
            .spacing(8)
            .width(300)
            .into()
    }

    fn view_source_library_content(&self) -> Element<'_, Message> {
        let import_btn =
            button(text("Import").size(14)).on_press(Message::OpenFileDialog);

        let assets = self.project.source_library.assets();
        let mut grid_rows: Vec<Element<'_, Message>> = Vec::new();

        // Build 2-column grid
        let mut i = 0;
        while i < assets.len() {
            let card1 = self.view_source_card(&assets[i]);
            if i + 1 < assets.len() {
                let card2 = self.view_source_card(&assets[i + 1]);
                grid_rows.push(row![card1, card2].spacing(6).into());
            } else {
                grid_rows.push(
                    row![card1, Space::new().width(130)]
                        .spacing(6)
                        .into(),
                );
            }
            i += 2;
        }

        let asset_grid = scrollable(column(grid_rows).spacing(6));

        column![import_btn, asset_grid]
            .spacing(8)
            .into()
    }

    fn view_effects_browser(&self) -> Element<'_, Message> {
        let has_selection = self.selected_clip.is_some();
        let mut items: Vec<Element<'_, Message>> = Vec::new();

        for effect_type in EffectType::all_builtin() {
            let label = text(effect_type.display_name()).size(14).color(Color::WHITE);
            let mut add_btn = button(text("Add to Clip").size(12))
                .padding([4, 8]);
            if has_selection {
                add_btn = add_btn.on_press(Message::AddEffectToSelectedClip(effect_type));
            }
            items.push(
                row![label, Space::new().width(Length::Fill), add_btn]
                    .spacing(8)
                    .align_y(iced::Alignment::Center)
                    .padding([4, 0])
                    .into(),
            );
        }

        if items.is_empty() {
            column![text("No effects available").size(14).color(Color::from_rgb(0.5, 0.5, 0.5))]
                .into()
        } else {
            scrollable(column(items).spacing(4)).into()
        }
    }

    fn view_clip_effects_inspector(&self) -> Element<'_, Message> {
        let (track_index, clip_id) = match self.selected_clip {
            Some(sel) => sel,
            None => {
                return container(
                    text("No clip selected").size(13).color(Color::from_rgb(0.5, 0.5, 0.5))
                )
                .width(250)
                .padding(8)
                .into();
            }
        };

        let clip = self.project.timeline.track(track_index)
            .ok()
            .and_then(|t| t.get_clip(clip_id));

        let clip = match clip {
            Some(c) => c,
            None => {
                return container(
                    text("Clip not found").size(13).color(Color::from_rgb(0.5, 0.5, 0.5))
                )
                .width(250)
                .padding(8)
                .into();
            }
        };

        let title = text("Clip Effects").size(16).color(Color::WHITE);
        let mut items: Vec<Element<'_, Message>> = vec![title.into()];

        if clip.effects.is_empty() {
            items.push(
                text("No effects").size(13).color(Color::from_rgb(0.5, 0.5, 0.5)).into()
            );
        } else {
            for effect in &clip.effects {
                let effect_name = text(effect.effect_type.display_name())
                    .size(14)
                    .color(Color::from_rgb(0.9, 0.9, 0.9));

                let remove_btn = button(text("Remove").size(11))
                    .on_press(Message::RemoveEffectFromClip {
                        track_index,
                        clip_id,
                        effect_id: effect.id,
                    })
                    .padding([2, 6])
                    .style(|_theme, _status| button::Style {
                        background: Some(Background::Color(Color::from_rgb(0.6, 0.2, 0.2))),
                        text_color: Color::WHITE,
                        border: Border { radius: 3.0.into(), ..Default::default() },
                        ..Default::default()
                    });

                items.push(
                    row![effect_name, Space::new().width(Length::Fill), remove_btn]
                        .align_y(iced::Alignment::Center)
                        .into()
                );

                // Parameter inputs
                for def in effect.effect_type.parameter_definitions() {
                    let current_val = effect.get_float(&def.name).unwrap_or(0.0);
                    let effect_id = effect.id;
                    let param_name = def.name.clone();

                    let zeditor_core::effects::ParameterType::Float { min, max, .. } = def.param_type;

                    // Use percentage display for 0-1 range parameters
                    let is_percentage = min == 0.0 && max == 1.0;
                    let wide_range = (max - min) > 100.0;

                    let param_label = if is_percentage {
                        text(format!("{}: {:.0}%", def.label, current_val * 100.0))
                            .size(12)
                            .color(Color::from_rgb(0.7, 0.7, 0.7))
                    } else if wide_range {
                        // Label only (value shown in the text input)
                        text(format!("{}:", def.label))
                            .size(12)
                            .color(Color::from_rgb(0.7, 0.7, 0.7))
                    } else {
                        let val_str = if current_val == current_val.trunc() {
                            format!("{}: {:.0}", def.label, current_val)
                        } else {
                            format!("{}: {:.2}", def.label, current_val)
                        };
                        text(val_str).size(12).color(Color::from_rgb(0.7, 0.7, 0.7))
                    };

                    let param_control: Element<'_, Message> = if wide_range {
                        // Wide-range params get a text input for precise entry
                        let text_key = (effect_id, def.name.clone());
                        let display_text = self.effect_param_texts
                            .get(&text_key)
                            .cloned()
                            .unwrap_or_else(|| {
                                if current_val == current_val.trunc() {
                                    format!("{:.0}", current_val)
                                } else {
                                    format!("{:.2}", current_val)
                                }
                            });
                        let param_name_for_input = param_name.clone();
                        text_input("0", &display_text)
                            .on_input(move |t| {
                                Message::EffectParamTextInput {
                                    track_index,
                                    clip_id,
                                    effect_id,
                                    param_name: param_name_for_input.clone(),
                                    text: t,
                                }
                            })
                            .width(80)
                            .size(12)
                            .into()
                    } else {
                        slider(min..=max, current_val, move |v| {
                            Message::UpdateEffectParameter {
                                track_index,
                                clip_id,
                                effect_id,
                                param_name: param_name.clone(),
                                value: v,
                            }
                        })
                        .step(if is_percentage { 0.01 } else { 0.01 })
                        .width(140)
                        .into()
                    };

                    items.push(
                        column![param_label, param_control]
                            .spacing(2)
                            .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 8.0 })
                            .into()
                    );
                }
            }
        }

        container(scrollable(column(items).spacing(6)))
            .width(250)
            .padding(8)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(0.16, 0.16, 0.18))),
                border: Border {
                    color: Color::from_rgb(0.25, 0.25, 0.28),
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn view_source_card<'a>(&'a self, asset: &'a zeditor_core::media::MediaAsset) -> Element<'a, Message> {
        let is_hovered = self.hovered_asset_id == Some(asset.id);
        let is_selected = self.selected_asset_id == Some(asset.id);
        let asset_id = asset.id;

        // Thumbnail or placeholder
        let thumb_content: Element<'_, Message> = if let Some(handle) = self.thumbnails.get(&asset_id) {
            image(handle.clone())
                .width(120)
                .height(68)
                .content_fit(iced::ContentFit::Contain)
                .into()
        } else {
            container(center(text("...").size(14).color(Color::from_rgb(0.5, 0.5, 0.5))))
                .width(120)
                .height(68)
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color::from_rgb(0.2, 0.2, 0.22))),
                    ..Default::default()
                })
                .into()
        };

        let border_color = if is_selected {
            Color::from_rgb(1.0, 0.2, 0.2)
        } else if is_hovered {
            Color::from_rgb(0.3, 0.5, 0.9)
        } else {
            Color::TRANSPARENT
        };

        let name_label = text(&asset.name)
            .size(11)
            .color(Color::WHITE)
            .width(120)
            .center();

        let card = container(
            column![thumb_content, name_label].spacing(2).align_x(iced::Alignment::Center),
        )
        .padding(4)
        .width(130)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.18, 0.18, 0.20))),
            border: Border {
                color: border_color,
                width: 2.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        mouse_area(card)
            .on_enter(Message::SourceCardHovered(Some(asset_id)))
            .on_exit(Message::SourceCardHovered(None))
            .on_press(Message::StartDragFromSource(asset_id))
            .on_release(Message::SelectSourceAsset(Some(asset_id)))
            .into()
    }

    fn view_video_viewport(&self) -> Element<'_, Message> {
        let play_pause = if self.is_playing {
            button(text("Pause").size(14)).on_press(Message::Pause)
        } else {
            button(text("Play").size(14)).on_press(Message::Play)
        };

        let position = text(format!(
            "{:.1}s",
            self.playback_position.as_secs_f64()
        ))
        .size(14);

        // Compute inner canvas width from project aspect ratio to show
        // letterboxing/pillarboxing that approximates the rendered output.
        let viewport_height: f32 = 300.0;
        let canvas_aspect = self.project.settings.canvas_width as f32
            / self.project.settings.canvas_height as f32;
        let canvas_preview_width = (viewport_height * canvas_aspect).round();

        let video_area: Element<'_, Message> = if let Some(handle) = &self.current_frame {
            // Black inner container at canvas aspect ratio with the video inside
            let inner = container(
                iced::widget::image(handle.clone())
                    .content_fit(iced::ContentFit::Contain)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .width(canvas_preview_width)
            .height(viewport_height)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::BLACK)),
                ..Default::default()
            });
            // Outer dark container centers the inner canvas box
            container(center(inner))
                .width(Length::Fill)
                .height(viewport_height)
                .style(container::dark)
                .into()
        } else {
            // Black canvas box with "No video" text
            let inner = container(center(text("No video").size(16)))
                .width(canvas_preview_width)
                .height(viewport_height)
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color::BLACK)),
                    ..Default::default()
                });
            container(center(inner))
                .width(Length::Fill)
                .height(viewport_height)
                .style(container::dark)
                .into()
        };

        let controls = row![play_pause, position].spacing(8);

        column![video_area, controls]
            .spacing(4)
            .width(Length::Fill)
            .into()
    }

    fn view_timeline(&self) -> Element<'_, Message> {
        let undo_btn = button(text("Undo").size(12)).on_press(Message::Undo);
        let redo_btn = button(text("Redo").size(12)).on_press(Message::Redo);
        let controls = row![undo_btn, redo_btn].spacing(5);

        // Compute source drag preview for the canvas
        let source_drag = self.compute_source_drag_preview();

        let canvas = iced::widget::canvas(TimelineCanvas {
            timeline: &self.project.timeline,
            playback_position: self.playback_position,
            selected_clip: self.selected_clip,
            zoom: self.timeline_zoom,
            scroll_offset: self.timeline_scroll,
            tool_mode: self.tool_mode,
            source_drag,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        // Track headers column (fixed width, left of canvas)
        let header_width: f32 = 60.0;
        let ruler_height: f32 = 20.0;
        let track_height: f32 = 50.0;

        let mut header_items: Vec<Element<'_, Message>> = Vec::new();
        // Ruler-height spacer at top to align with canvas ruler
        header_items.push(
            container(text("").size(1))
                .height(ruler_height)
                .width(header_width)
                .into()
        );

        for (i, track) in self.project.timeline.tracks.iter().enumerate() {
            let bg_color = match track.track_type {
                TrackType::Video => Color::from_rgb(0.16, 0.18, 0.16),
                TrackType::Audio => Color::from_rgb(0.14, 0.16, 0.20),
            };
            let header = container(
                text(&track.name)
                    .size(12)
                    .color(Color::from_rgb(0.8, 0.8, 0.8))
            )
            .padding([4, 6])
            .width(header_width)
            .height(track_height)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(bg_color)),
                border: Border {
                    color: Color::from_rgb(0.3, 0.3, 0.35),
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });
            // Wrap in mouse_area to detect right-click for context menu
            let header_with_menu: Element<'_, Message> = mouse_area(header)
                .on_right_press(Message::ShowTrackContextMenu {
                    track_index: i,
                    screen_position: Point::new(
                        header_width,
                        ruler_height + i as f32 * track_height,
                    ),
                })
                .into();
            header_items.push(header_with_menu);
        }

        let header_col: Element<'_, Message> = column(header_items).into();

        let timeline_row: Element<'_, Message> = row![header_col, canvas]
            .height(Length::Fill)
            .into();

        let timeline_content: Element<'_, Message> = column![controls, timeline_row]
            .spacing(4)
            .height(Length::Fill)
            .into();

        // When dragging, wrap timeline in mouse_area for enter/exit/move detection
        let timeline_content: Element<'_, Message> = if self.drag_state.is_some() {
            mouse_area(timeline_content)
                .on_enter(Message::DragEnteredTimeline)
                .on_exit(Message::DragExitedTimeline)
                .on_move(Message::DragOverTimeline)
                .into()
        } else {
            timeline_content
        };

        // Add context menu overlay if present
        if let Some(ref ctx) = self.track_context_menu {
            let track_type = ctx.track_type;
            let track_index = ctx.track_index;
            let menu_y = ctx.position.y;
            let menu_x = ctx.position.x;

            let click_off: Element<'_, Message> = mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_press(Message::DismissTrackContextMenu)
            .into();

            let menu_items: Vec<Element<'_, Message>> = match track_type {
                TrackType::Video => vec![
                    self.context_menu_item("Add Video Track Above", Message::AddVideoTrackAbove(track_index)),
                    self.context_menu_item("Add Video Track Below", Message::AddVideoTrackBelow(track_index)),
                ],
                TrackType::Audio => vec![
                    self.context_menu_item("Add Audio Track Above", Message::AddAudioTrackAbove(track_index)),
                    self.context_menu_item("Add Audio Track Below", Message::AddAudioTrackBelow(track_index)),
                ],
            };

            let menu = container(column(menu_items).spacing(0))
                .width(200)
                .padding(4)
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color::from_rgb(0.22, 0.22, 0.24))),
                    border: Border {
                        color: Color::from_rgb(0.15, 0.15, 0.17),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                });

            let positioned_menu = container(menu)
                .padding(Padding {
                    top: menu_y.max(0.0),
                    left: menu_x.max(0.0),
                    right: 0.0,
                    bottom: 0.0,
                })
                .width(Length::Fill)
                .height(Length::Fill);

            stack![timeline_content, opaque(click_off), positioned_menu]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            timeline_content
        }
    }

    fn context_menu_item<'a>(&self, label: &'a str, msg: Message) -> Element<'a, Message> {
        button(
            text(label).size(13).color(Color::WHITE).width(Length::Fill),
        )
        .on_press(msg)
        .width(Length::Fill)
        .padding([6, 10])
        .style(|_theme, status| {
            let bg = if matches!(status, button::Status::Hovered) {
                Color::from_rgb(0.32, 0.32, 0.35)
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
    }

    fn compute_source_drag_preview(&self) -> Option<SourceDragPreview> {
        let drag = self.drag_state.as_ref()?;
        if !drag.over_timeline {
            return None;
        }
        let (asset_id, track_index, position) = match &drag.payload {
            DragPayload::SourceAsset { asset_id, .. } => {
                let track_index = drag.timeline_track?;
                let position = drag.timeline_position?;
                (*asset_id, track_index, position)
            }
        };
        let asset = self.project.source_library.get(asset_id)?;
        let audio_track_index = if asset.has_audio {
            self.project.timeline.find_paired_audio_track(track_index)
        } else {
            None
        };
        Some(SourceDragPreview {
            asset_id,
            duration_secs: asset.duration.as_secs_f64(),
            track_index,
            position,
            audio_track_index,
        })
    }

    fn view_menu_bar(&self) -> Element<'_, Message> {
        let file_btn = self.menu_bar_button("File", MenuId::File);
        let edit_btn = self.menu_bar_button("Edit", MenuId::Edit);

        container(
            row![file_btn, edit_btn].spacing(2).padding([2, 4]),
        )
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.20, 0.20, 0.22))),
            border: Border {
                color: Color::from_rgb(0.15, 0.15, 0.17),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
    }

    fn menu_bar_button<'a>(&self, label: &'a str, menu_id: MenuId) -> Element<'a, Message> {
        let is_active = self.open_menu == Some(menu_id);

        let btn = button(text(label).size(14).color(Color::WHITE))
            .on_press(Message::MenuButtonClicked(menu_id))
            .padding([4, 10])
            .style(move |_theme, status| {
                let bg = if is_active {
                    Color::from_rgb(0.35, 0.35, 0.38)
                } else if matches!(status, button::Status::Hovered) {
                    Color::from_rgb(0.30, 0.30, 0.33)
                } else {
                    Color::TRANSPARENT
                };
                button::Style {
                    background: Some(Background::Color(bg)),
                    text_color: Color::WHITE,
                    border: Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            });

        mouse_area(btn)
            .on_enter(Message::MenuButtonHovered(menu_id))
            .into()
    }

    fn view_dropdown(&self) -> Element<'_, Message> {
        let menu_id = self.open_menu.unwrap_or(MenuId::File);

        let items: Vec<Element<'_, Message>> = match menu_id {
            MenuId::File => vec![
                self.menu_item("New Project", MenuAction::NewProject),
                self.menu_item("Load Project", MenuAction::LoadProject),
                self.menu_item("Save", MenuAction::Save),
                self.menu_item("Render", MenuAction::Render),
                self.menu_item("Exit", MenuAction::Exit),
            ],
            MenuId::Edit => vec![
                self.menu_item("Undo", MenuAction::Undo),
                self.menu_item("Redo", MenuAction::Redo),
            ],
        };

        let left_offset: f32 = match menu_id {
            MenuId::File => 8.0,
            MenuId::Edit => 58.0,
        };

        let dropdown = container(column(items).spacing(0))
            .width(180)
            .padding(4)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(0.22, 0.22, 0.24))),
                border: Border {
                    color: Color::from_rgb(0.15, 0.15, 0.17),
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            });

        // Position at top-left of content area with left offset
        container(dropdown)
            .padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: left_offset,
            })
            .into()
    }

    fn menu_item<'a>(&self, label: &'a str, action: MenuAction) -> Element<'a, Message> {
        button(
            text(label).size(14).color(Color::WHITE).width(Length::Fill),
        )
        .on_press(Message::MenuAction(action))
        .width(Length::Fill)
        .padding([6, 12])
        .style(|_theme, status| {
            let bg = if matches!(status, button::Status::Hovered) {
                Color::from_rgb(0.32, 0.32, 0.35)
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
    }

    fn view_drag_overlay<'a>(&'a self, drag: &'a DragState) -> Element<'a, Message> {
        let (thumbnail, name) = match &drag.payload {
            DragPayload::SourceAsset { thumbnail, name, .. } => (thumbnail.clone(), name.as_str()),
        };

        let ghost_alpha = 0.3;

        let thumb_content: Element<'_, Message> = if let Some(handle) = thumbnail {
            image(handle)
                .width(120)
                .height(68)
                .content_fit(iced::ContentFit::Contain)
                .opacity(ghost_alpha)
                .into()
        } else {
            container(center(text("...").size(14).color(Color { r: 0.5, g: 0.5, b: 0.5, a: ghost_alpha })))
                .width(120)
                .height(68)
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(Color { r: 0.2, g: 0.2, b: 0.22, a: ghost_alpha })),
                    ..Default::default()
                })
                .into()
        };

        let name_label = text(name)
            .size(11)
            .color(Color { r: 1.0, g: 1.0, b: 1.0, a: ghost_alpha })
            .width(120)
            .center();

        let card = container(
            column![thumb_content, name_label].spacing(2).align_x(iced::Alignment::Center),
        )
        .padding(4)
        .width(130)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(Color {
                r: 0.18,
                g: 0.18,
                b: 0.20,
                a: ghost_alpha,
            })),
            border: Border {
                color: Color { r: 0.3, g: 0.5, b: 0.9, a: ghost_alpha },
                width: 2.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        // Position at cursor with offset so card is centered on cursor
        let cursor = drag.cursor_position;
        container(card)
            .padding(Padding {
                top: (cursor.y - 45.0).max(0.0),
                left: (cursor.x - 65.0).max(0.0),
                right: 0.0,
                bottom: 0.0,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Find the video clip at the given playback position (searches only video tracks).
    pub fn clip_at_position(&self, pos: TimelinePosition) -> Option<(usize, &Clip)> {
        for (i, track) in self.project.timeline.tracks.iter().enumerate() {
            if track.track_type == TrackType::Video {
                if let Some(clip) = track.clip_at(pos) {
                    return Some((i, clip));
                }
            }
        }
        None
    }

    /// Find ALL video clips at the given playback position, ordered bottom-to-top (V1 first, VN last).
    /// Video tracks are stored top-to-bottom in the vec (VN...V1), so we iterate in reverse.
    pub fn all_video_clips_at_position(&self, pos: TimelinePosition) -> Vec<(usize, &Clip)> {
        let mut clips = Vec::new();
        let video_tracks: Vec<_> = self.project.timeline.tracks.iter().enumerate()
            .filter(|(_, t)| t.track_type == TrackType::Video)
            .collect();
        for (i, track) in video_tracks.iter().rev() {
            if let Some(clip) = track.clip_at(pos) {
                clips.push((*i, clip));
            }
        }
        clips
    }

    /// Find ALL audio clips at the given playback position, from all audio tracks.
    pub fn all_audio_clips_at_position(&self, pos: TimelinePosition) -> Vec<(usize, &Clip)> {
        let mut clips = Vec::new();
        for (i, track) in self.project.timeline.tracks.iter().enumerate() {
            if track.track_type == TrackType::Audio {
                if let Some(clip) = track.clip_at(pos) {
                    clips.push((i, clip));
                }
            }
        }
        clips
    }

    /// Find the audio clip at the given playback position (searches only audio tracks).
    pub fn audio_clip_at_position(&self, pos: TimelinePosition) -> Option<(usize, &Clip)> {
        for (i, track) in self.project.timeline.tracks.iter().enumerate() {
            if track.track_type == TrackType::Audio {
                if let Some(clip) = track.clip_at(pos) {
                    return Some((i, clip));
                }
            }
        }
        None
    }

    /// Send a seek request to the decode thread for the current playback position.
    /// If `continuous` is true, the thread decodes ahead (playback mode).
    /// If false, it decodes one target frame and stops (scrub mode).
    fn send_decode_seek(&mut self, continuous: bool) {
        // Drain stale frames from the channel and discard pending
        self.pending_frame = None;
        if let Some(rx) = &self.decode_rx {
            while rx.try_recv().is_ok() {}
        }

        // Collect clip info upfront to avoid borrow conflicts with self.
        // Video tracks are stored top-to-bottom (VN...V1), iterate in reverse for bottom-to-top.
        let playback_pos = self.playback_position.as_secs_f64();
        let mut clip_infos = Vec::new();
        let mut clip_ids = Vec::new();
        let mut first_clip_id = None;
        let mut first_time_offset = 0.0;

        let video_track_indices: Vec<usize> = self.project.timeline.tracks.iter()
            .enumerate()
            .filter(|(_, t)| t.track_type == TrackType::Video)
            .map(|(i, _)| i)
            .collect();

        for &idx in video_track_indices.iter().rev() {
            let track = &self.project.timeline.tracks[idx];
            if let Some(clip) = track.clip_at(self.playback_position) {
                let clip_tl_start = clip.timeline_range.start.as_secs_f64();
                let clip_src_start = clip.source_range.start.as_secs_f64();
                let source_time = clip_src_start + (playback_pos - clip_tl_start);
                if let Some(asset) = self.project.source_library.get(clip.asset_id) {
                    if first_clip_id.is_none() {
                        first_clip_id = Some(clip.id);
                        first_time_offset = clip_tl_start - clip_src_start;
                    }
                    clip_ids.push(clip.id);
                    clip_infos.push(ClipDecodeInfo {
                        path: asset.path.clone(),
                        time: source_time,
                        effects: clip.effects.clone(),
                    });
                }
            }
        }

        if clip_infos.is_empty() {
            self.decode_clip_id = None;
            self.decode_clip_ids.clear();
            self.current_frame = None;
            self.pending_frame = None;
            self.send_decode_stop();
            return;
        }

        self.decode_clip_id = first_clip_id;
        self.decode_clip_ids = clip_ids;
        self.decode_time_offset = first_time_offset;

        if let Some(tx) = &self.decode_tx {
            let _ = tx.send(DecodeRequest::SeekMulti {
                clips: clip_infos,
                continuous,
                canvas_w: self.project.settings.canvas_width,
                canvas_h: self.project.settings.canvas_height,
            });
        }
    }

    /// Tell the decode thread to stop decoding.
    fn send_decode_stop(&self) {
        if let Some(tx) = &self.decode_tx {
            let _ = tx.send(DecodeRequest::Stop);
        }
    }

    /// Display decoded frames that are due according to the playback clock.
    /// Holds frames whose PTS is ahead of the current playback position.
    fn poll_decoded_frame(&mut self) {
        // If no clip is being decoded, drain any stale frames from the channel
        // and ensure the display is black. This prevents a race where the decode
        // worker sends one last frame after send_decode_seek drained the channel.
        if self.decode_clip_id.is_none() {
            self.pending_frame = None;
            self.current_frame = None;
            if let Some(rx) = &self.decode_rx {
                while rx.try_recv().is_ok() {}
            }
            return;
        }

        let playback_secs = self.playback_position.as_secs_f64();

        loop {
            // Get a frame: either from pending or from channel
            let frame = if self.pending_frame.is_some() {
                self.pending_frame.take().unwrap()
            } else if let Some(rx) = &self.decode_rx {
                match rx.try_recv() {
                    Ok(f) => f,
                    Err(_) => return,
                }
            } else {
                return;
            };

            // Convert source PTS to timeline time
            let frame_timeline_time = frame.pts_secs + self.decode_time_offset;

            // When paused (scrubbing), always display immediately.
            // When playing, only display if the frame's time has arrived.
            if !self.is_playing || frame_timeline_time <= playback_secs + 0.02 {
                // Frame is already composited by the decode worker (multi-clip + transforms)
                self.current_frame = Some(iced::widget::image::Handle::from_rgba(
                    frame.width, frame.height, frame.rgba,
                ));
                self.drain_stale = false;
                // Loop to check if there's an even more recent frame also due
            } else if self.drain_stale {
                // After a decode transition, frames that are too far ahead are
                // stale leftovers from the old context (wrong time offset).
                // Discard them and try the next frame from the channel.
                continue;
            } else {
                // Frame is ahead of playback — hold it for a future tick
                self.pending_frame = Some(frame);
                return;
            }
        }
    }
    /// Send a seek request to the audio decode thread for the current playback position.
    fn send_audio_decode_seek(&mut self, continuous: bool) {
        // Drain stale audio from the channel
        if let Some(rx) = &self.audio_decode_rx {
            while rx.try_recv().is_ok() {}
        }

        // Collect clip info upfront to avoid borrow conflicts with self
        let playback_pos = self.playback_position.as_secs_f64();
        let mut audio_infos = Vec::new();
        let mut clip_ids = Vec::new();
        let mut first_time_offset = 0.0;
        let mut got_first = false;

        for (_, track) in self.project.timeline.tracks.iter().enumerate() {
            if track.track_type != TrackType::Audio {
                continue;
            }
            if let Some(clip) = track.clip_at(self.playback_position) {
                let clip_tl_start = clip.timeline_range.start.as_secs_f64();
                let clip_src_start = clip.source_range.start.as_secs_f64();
                let source_time = clip_src_start + (playback_pos - clip_tl_start);

                if let Some(asset) = self.project.source_library.get(clip.asset_id) {
                    if !got_first {
                        first_time_offset = clip_tl_start - clip_src_start;
                        got_first = true;
                    }
                    clip_ids.push(clip.id);
                    audio_infos.push(AudioClipInfo {
                        path: asset.path.clone(),
                        time: source_time,
                    });
                }
            }
        }

        if audio_infos.is_empty() {
            self.audio_decode_clip_id = None;
            self.send_audio_decode_stop();
            if let Some(player) = &self.audio_player {
                player.stop();
            }
            return;
        }

        // Clear buffered audio from previous clips before starting new ones.
        if let Some(player) = &self.audio_player {
            player.clear();
            player.play();
        }
        self.audio_decode_clip_id = clip_ids.into_iter().next();
        self.audio_decode_time_offset = first_time_offset;

        if let Some(tx) = &self.audio_decode_tx {
            let _ = tx.send(AudioDecodeRequest::SeekMulti {
                clips: audio_infos,
                continuous,
            });
        }
    }

    /// Tell the audio decode thread to stop.
    fn send_audio_decode_stop(&self) {
        if let Some(tx) = &self.audio_decode_tx {
            let _ = tx.send(AudioDecodeRequest::Stop);
        }
    }

    /// Drain decoded audio frames and feed them to the audio player.
    fn poll_decoded_audio(&mut self) {
        if self.audio_decode_clip_id.is_none() {
            if let Some(rx) = &self.audio_decode_rx {
                while rx.try_recv().is_ok() {}
            }
            return;
        }

        if let Some(rx) = &self.audio_decode_rx {
            while let Ok(audio) = rx.try_recv() {
                if let Some(player) = &self.audio_player {
                    player.queue_audio(audio.samples, audio.sample_rate, audio.channels);
                }
            }
        }
    }
}

/// Convert RGB24 pixel data to RGBA32 (adds alpha=255).
pub fn rgb24_to_rgba32(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let pixel_count = (width * height) as usize;
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    for i in 0..pixel_count {
        rgba.push(rgb[i * 3]);
        rgba.push(rgb[i * 3 + 1]);
        rgba.push(rgb[i * 3 + 2]);
        rgba.push(255);
    }
    rgba
}

/// Nearest-neighbor scale + blit RGBA pixels onto a destination buffer.
/// Handles negative offsets and out-of-bounds clipping.
pub fn blit_rgba_scaled(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    offset_x: i32,
    offset_y: i32,
    clip_w: u32,
    clip_h: u32,
) {
    if clip_w == 0 || clip_h == 0 {
        return;
    }

    // Visible region in dst coords.
    let x0 = offset_x.max(0) as u32;
    let y0 = offset_y.max(0) as u32;
    let x1 = ((offset_x + clip_w as i32) as u32).min(dst_w);
    let y1 = ((offset_y + clip_h as i32) as u32).min(dst_h);

    if x0 >= x1 || y0 >= y1 {
        return;
    }

    for dy in y0..y1 {
        // Map dst y to src y via nearest-neighbor.
        let local_y = (dy as i32 - offset_y) as u32;
        let sy = ((local_y as u64 * src_h as u64) / clip_h as u64).min(src_h as u64 - 1) as u32;
        for dx in x0..x1 {
            let local_x = (dx as i32 - offset_x) as u32;
            let sx =
                ((local_x as u64 * src_w as u64) / clip_w as u64).min(src_w as u64 - 1) as u32;
            let si = (sy * src_w + sx) as usize * 4;
            let di = (dy * dst_w + dx) as usize * 4;
            dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
}

/// Event filter for global mouse tracking during drag operations.
/// Plain function pointer (not closure) as required by `event::listen_with`.
fn drag_event_filter(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::DragMoved(position))
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::DragReleased)
        }
        _ => None,
    }
}

/// Cached decoder state for the decode worker thread.
struct CachedDecoder {
    decoder: zeditor_media::decoder::FfmpegDecoder,
    last_pts: f64,
}

/// Background decode worker thread. Owns the FFmpeg decoder and runs ahead of playback.
/// Supports both single-clip (legacy Seek) and multi-clip (SeekMulti) decode modes.
fn decode_worker(
    request_rx: mpsc::Receiver<DecodeRequest>,
    frame_tx: mpsc::SyncSender<DecodedFrame>,
) {
    use zeditor_media::decoder::{FfmpegDecoder, VideoDecoder};

    let registry = EffectRegistry::with_builtins();
    let mut decoders: HashMap<PathBuf, CachedDecoder> = HashMap::new();
    let mut running = false;
    let mut is_continuous = false;
    let mut target_time: f64 = 0.0;
    let mut seeking_to_target = false;
    let mut multi_clips: Vec<ClipDecodeInfo> = Vec::new();
    let mut multi_canvas_w: u32 = 1920;
    let mut multi_canvas_h: u32 = 1080;

    loop {
        let request = if running {
            match request_rx.try_recv() {
                Ok(req) => Some(req),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        } else {
            match request_rx.recv() {
                Ok(req) => Some(req),
                Err(_) => return,
            }
        };

        if let Some(request) = request {
            match request {
                DecodeRequest::SeekMulti {
                    clips,
                    continuous,
                    canvas_w,
                    canvas_h,
                } => {
                    multi_canvas_w = canvas_w;
                    multi_canvas_h = canvas_h;

                    // Open/seek all decoders
                    let mut ok = true;
                    for clip in &clips {
                        if !decoders.contains_key(&clip.path) {
                            match FfmpegDecoder::open(&clip.path) {
                                Ok(decoder) => {
                                    decoders.insert(clip.path.clone(), CachedDecoder {
                                        decoder,
                                        last_pts: -1.0,
                                    });
                                }
                                Err(_) => { ok = false; break; }
                            }
                        }
                        let cached = decoders.get_mut(&clip.path).unwrap();
                        let needs_seek = clip.time < cached.last_pts
                            || (clip.time - cached.last_pts) > 2.0
                            || cached.last_pts < 0.0;
                        if needs_seek {
                            if cached.decoder.seek_to(clip.time).is_err() {
                                ok = false;
                                break;
                            }
                            cached.last_pts = -1.0;
                        }
                    }
                    if !ok {
                        running = false;
                        continue;
                    }
                    multi_clips = clips;
                    target_time = multi_clips.first().map(|c| c.time).unwrap_or(0.0);
                    seeking_to_target = true;
                    is_continuous = continuous;
                    running = true;
                }
                DecodeRequest::Stop => {
                    running = false;
                    continue;
                }
            }
        }

        if !running {
            continue;
        }

        // Decode one frame from each clip, composite, and send
        let result = decode_and_composite_multi(
            &multi_clips,
            &mut decoders,
            multi_canvas_w,
            multi_canvas_h,
            target_time,
            seeking_to_target,
            &registry,
        );
        match result {
            Ok(Some(frame)) => {
                seeking_to_target = false;
                if frame_tx.send(frame).is_err() {
                    return;
                }
                if !is_continuous {
                    running = false;
                } else {
                    // Advance target time slightly for next frame
                    target_time += 1.0 / 30.0; // approximate frame step
                }
            }
            Ok(None) => {
                if seeking_to_target {
                    // Still seeking, try again
                    continue;
                }
                running = false;
            }
            Err(_) => {
                running = false;
            }
        }
    }
}

/// Decode one frame from each clip and composite them into a single RGBA frame.
/// Clips are ordered bottom-to-top (V1 first, VN last).
/// Returns Ok(None) if all clips are at EOF.
///
/// Clips with effects go through the pixel pipeline (decode → canvas buffer →
/// effects → alpha composite). Clips without effects use the fast path
/// (decode → direct blit, same as before).
fn decode_and_composite_multi(
    clips: &[ClipDecodeInfo],
    decoders: &mut HashMap<PathBuf, CachedDecoder>,
    canvas_w: u32,
    canvas_h: u32,
    _target_time: f64,
    seeking: bool,
    registry: &EffectRegistry,
) -> std::result::Result<Option<DecodedFrame>, ()> {
    // Determine preview canvas size (fit canvas aspect ratio within PREVIEW_MAX).
    let scale_x = PREVIEW_MAX_WIDTH as f64 / canvas_w as f64;
    let scale_y = PREVIEW_MAX_HEIGHT as f64 / canvas_h as f64;
    let preview_scale = scale_x.min(scale_y).min(1.0);
    let pw = (canvas_w as f64 * preview_scale).round() as u32;
    let ph = (canvas_h as f64 * preview_scale).round() as u32;

    // Output canvas (black, opaque for background)
    let mut canvas_buf = FrameBuffer::new(pw, ph);
    // Fill with opaque black background
    for pixel in canvas_buf.data.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    let mut any_decoded = false;
    let mut first_pts = 0.0_f64;

    let ctx = EffectContext {
        time_secs: _target_time,
        frame_number: 0,
        fps: 30.0,
    };

    for (i, clip) in clips.iter().enumerate() {
        let cached = match decoders.get_mut(&clip.path) {
            Some(c) => c,
            None => continue,
        };

        // Decode frames until we get one at or past target
        let frame = loop {
            match cached.decoder.decode_next_frame_rgba_scaled(PREVIEW_MAX_WIDTH, PREVIEW_MAX_HEIGHT) {
                Ok(Some(f)) => {
                    cached.last_pts = f.pts_secs;
                    if seeking && f.pts_secs < clip.time - 0.05 {
                        continue; // skip pre-target frames
                    }
                    break Some(f);
                }
                Ok(None) => break None,
                Err(_) => break None,
            }
        };

        if let Some(frame) = frame {
            if i == 0 {
                first_pts = frame.pts_secs;
            }

            if clip.effects.is_empty() {
                // Fast path: no effects, direct blit (opaque overwrite)
                let fit_scale_x = canvas_w as f64 / frame.width as f64;
                let fit_scale_y = canvas_h as f64 / frame.height as f64;
                let fit_scale = fit_scale_x.min(fit_scale_y);
                let clip_w_canvas = (frame.width as f64 * fit_scale).round();
                let clip_h_canvas = (frame.height as f64 * fit_scale).round();
                let center_x_canvas = (canvas_w as f64 - clip_w_canvas) / 2.0;
                let center_y_canvas = (canvas_h as f64 - clip_h_canvas) / 2.0;

                let offset_x = (center_x_canvas * preview_scale).round() as i32;
                let offset_y = (center_y_canvas * preview_scale).round() as i32;
                let clip_w = (clip_w_canvas * preview_scale).round() as u32;
                let clip_h = (clip_h_canvas * preview_scale).round() as u32;

                blit_rgba_scaled(
                    &frame.data, frame.width, frame.height,
                    &mut canvas_buf.data, pw, ph,
                    offset_x, offset_y, clip_w, clip_h,
                );
            } else {
                // Effect path: decode → pipeline (canvas buffer + effects) → smart composite
                let clip_frame = FrameBuffer::from_rgba_vec(
                    frame.width, frame.height, frame.data,
                );
                let result = pipeline::run_effect_pipeline(
                    clip_frame, pw, ph, &clip.effects, registry, &ctx,
                );
                if !result.may_have_transparency && result.fills_canvas {
                    pipeline::composite_opaque(&result.frame, &mut canvas_buf);
                } else {
                    pipeline::alpha_composite_rgba(&result.frame, &mut canvas_buf);
                }
            }
            any_decoded = true;
        }
    }

    if any_decoded {
        Ok(Some(DecodedFrame {
            rgba: canvas_buf.data,
            width: pw,
            height: ph,
            pts_secs: first_pts,
        }))
    } else {
        Ok(None)
    }
}

/// Cached audio decoder state for the audio decode worker thread.
struct CachedAudioDecoder {
    decoder: zeditor_media::audio_decoder::FfmpegAudioDecoder,
    path: PathBuf,
    last_pts: f64,
}

/// Background audio decode worker thread. Supports multi-clip mixing.
/// Uses per-clip-index decoders (not per-path) so overlapping clips from the
/// same source file each get their own independent decoder.
fn audio_decode_worker(
    request_rx: mpsc::Receiver<AudioDecodeRequest>,
    audio_tx: mpsc::SyncSender<DecodedAudio>,
) {
    use zeditor_media::audio_decoder::FfmpegAudioDecoder;

    let mut audio_decoders: Vec<Option<CachedAudioDecoder>> = Vec::new();
    let mut running = false;
    let mut is_continuous = false;
    let mut seeking_to_target = false;
    let mut multi_clips: Vec<AudioClipInfo> = Vec::new();

    loop {
        let request = if running {
            match request_rx.try_recv() {
                Ok(req) => Some(req),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        } else {
            match request_rx.recv() {
                Ok(req) => Some(req),
                Err(_) => return,
            }
        };

        if let Some(request) = request {
            match request {
                AudioDecodeRequest::SeekMulti {
                    clips,
                    continuous,
                } => {
                    // Resize decoder vec to match clip count
                    audio_decoders.resize_with(clips.len(), || None);
                    audio_decoders.truncate(clips.len());

                    // Open/seek decoders per clip index
                    let mut ok = true;
                    for (i, clip) in clips.iter().enumerate() {
                        // Reuse existing decoder if it's for the same path
                        let needs_new = match &audio_decoders[i] {
                            Some(cached) => cached.path != clip.path,
                            None => true,
                        };
                        if needs_new {
                            match FfmpegAudioDecoder::open(&clip.path) {
                                Ok(decoder) => {
                                    audio_decoders[i] = Some(CachedAudioDecoder {
                                        decoder,
                                        path: clip.path.clone(),
                                        last_pts: -1.0,
                                    });
                                }
                                Err(_) => { ok = false; break; }
                            }
                        }
                        let cached = audio_decoders[i].as_mut().unwrap();
                        let needs_seek = clip.time < cached.last_pts
                            || (clip.time - cached.last_pts) > 2.0
                            || cached.last_pts < 0.0;
                        if needs_seek {
                            if cached.decoder.seek_to(clip.time).is_err() {
                                ok = false;
                                break;
                            }
                            cached.last_pts = -1.0;
                        }
                    }
                    if !ok {
                        running = false;
                        continue;
                    }
                    multi_clips = clips;
                    seeking_to_target = true;
                    is_continuous = continuous;
                    running = true;
                }
                AudioDecodeRequest::Stop => {
                    running = false;
                    continue;
                }
            }
        }

        if !running {
            continue;
        }

        // Multi-clip: decode one frame from each, mix, send
        let mut mixed_samples: Option<Vec<f32>> = None;
        let mut first_pts = 0.0_f64;
        let mut sample_rate = 48000u32;
        let mut channels = 2u16;
        let mut any_decoded = false;

        for (i, clip) in multi_clips.iter().enumerate() {
            let cached = match audio_decoders.get_mut(i).and_then(|c| c.as_mut()) {
                Some(c) => c,
                None => continue,
            };

            let frame = loop {
                match cached.decoder.decode_next_audio_frame() {
                    Ok(Some(f)) => {
                        cached.last_pts = f.pts_secs;
                        if seeking_to_target && f.pts_secs < clip.time - 0.05 {
                            continue;
                        }
                        break Some(f);
                    }
                    Ok(None) => break None,
                    Err(_) => break None,
                }
            };

            if let Some(frame) = frame {
                if i == 0 {
                    first_pts = frame.pts_secs;
                    sample_rate = frame.sample_rate;
                    channels = frame.channels;
                }
                any_decoded = true;

                match &mut mixed_samples {
                    None => {
                        mixed_samples = Some(frame.samples);
                    }
                    Some(mixed) => {
                        // Additive mixing with clamping
                        let len = mixed.len().min(frame.samples.len());
                        for j in 0..len {
                            mixed[j] = (mixed[j] + frame.samples[j]).clamp(-1.0, 1.0);
                        }
                        // If this clip has more samples, extend
                        if frame.samples.len() > mixed.len() {
                            mixed.extend_from_slice(&frame.samples[mixed.len()..]);
                        }
                    }
                }
            }
        }

        seeking_to_target = false;

        if any_decoded {
            let decoded = DecodedAudio {
                samples: mixed_samples.unwrap_or_default(),
                sample_rate,
                channels,
                pts_secs: first_pts,
            };
            if audio_tx.send(decoded).is_err() {
                return;
            }
            if !is_continuous {
                running = false;
            }
        } else {
            running = false;
        }
    }
}
