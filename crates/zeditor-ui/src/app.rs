use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use std::collections::HashMap;

use iced::widget::{button, center, column, container, image, mouse_area, opaque, row, scrollable, stack, text, Space};
use iced::{event, keyboard, mouse, time, window, Background, Border, Color, Element, Event, Length, Padding, Point, Subscription, Task};
use uuid::Uuid;

use zeditor_core::project::Project;
use zeditor_core::timeline::{Clip, TimeRange, TimelinePosition, TrackType};

use crate::audio_player::AudioPlayer;
use crate::message::{ConfirmAction, ConfirmDialog, DragPayload, DragState, MenuAction, MenuId, Message, SourceDragPreview, ToolMode};
use crate::widgets::timeline_canvas::TimelineCanvas;

/// Preview resolution cap. 4K frames are scaled down to this for display.
const PREVIEW_MAX_WIDTH: u32 = 960;
const PREVIEW_MAX_HEIGHT: u32 = 540;

/// Request sent from UI to the decode thread.
enum DecodeRequest {
    /// Seek to time. If `continuous` is true, keep decoding (playback).
    /// If false, decode one target frame and stop (scrub).
    Seek {
        path: PathBuf,
        time: f64,
        continuous: bool,
    },
    Stop,
}

/// Request sent from UI to the audio decode thread.
enum AudioDecodeRequest {
    Seek {
        path: PathBuf,
        time: f64,
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
    decode_tx: Option<mpsc::Sender<DecodeRequest>>,
    pub(crate) decode_rx: Option<mpsc::Receiver<DecodedFrame>>,
    pub(crate) decode_clip_id: Option<Uuid>,
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
    pub(crate) audio_decode_clip_id: Option<Uuid>,
    pub(crate) audio_decode_time_offset: f64,
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
            decode_tx: None,
            decode_rx: None,
            decode_clip_id: None,
            decode_time_offset: 0.0,
            pending_frame: None,
            drain_stale: false,
            audio_player: None,
            audio_decode_tx: None,
            audio_decode_rx: None,
            audio_decode_clip_id: None,
            audio_decode_time_offset: 0.0,
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
        self.thumbnails.clear();
        self.drag_state = None;
        self.timeline_zoom = 100.0;
        self.timeline_scroll = 0.0;
        self.tool_mode = ToolMode::default();
        self.open_menu = None;
        self.decode_clip_id = None;
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
                                &path, 160, 90,
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
        let (audio_frame_tx, audio_frame_rx) = mpsc::sync_channel::<DecodedAudio>(4);

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
                                    &path, 160, 90,
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
                    // Account for controls row height (~30px) and ruler height (20px)
                    let controls_height = 30.0_f32;
                    let ruler_height = 20.0_f32;
                    let track_height = 50.0_f32;

                    let secs = ((point.x + self.timeline_scroll) / self.timeline_zoom) as f64;
                    let secs = secs.max(0.0);

                    let track_y = point.y - controls_height - ruler_height;
                    let track_index = if track_y < 0.0 {
                        0
                    } else {
                        let idx = (track_y / track_height) as usize;
                        idx.min(self.project.timeline.tracks.len().saturating_sub(1))
                    };

                    // Only place on video tracks
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

                    // Check if we've crossed into a different video clip
                    let current_clip_id =
                        self.clip_at_position(self.playback_position).map(|(_, c)| c.id);
                    if current_clip_id != self.decode_clip_id {
                        self.send_decode_seek(true);
                        self.drain_stale = true;
                    }

                    // Check if audio clip changed
                    let current_audio_id =
                        self.audio_clip_at_position(self.playback_position).map(|(_, c)| c.id);
                    if current_audio_id != self.audio_decode_clip_id {
                        self.send_audio_decode_seek(true);
                    }
                }

                // Drain decoded frames from the channels
                self.poll_decoded_frame();
                self.poll_decoded_audio();
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
                        Task::perform(
                            async move {
                                zeditor_media::renderer::render_timeline(
                                    &timeline,
                                    &source_library,
                                    &config,
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
                self.status_message = format!("Rendered to {}", path.display());
                Task::none()
            }
            Message::RenderError(msg) => {
                self.status_message = format!("Render failed: {msg}");
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
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let menu_bar = self.view_menu_bar();
        let source_panel = self.view_source_library();
        let video_viewport = self.view_video_viewport();
        let timeline_panel = self.view_timeline();

        let pos_secs = self.playback_position.as_secs_f64();
        let total_secs = pos_secs as u64;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        let millis = ((pos_secs - total_secs as f64) * 1000.0) as u64;
        let status = text(format!(
            "{} | {:02}:{:02}:{:02}.{:03} | Zoom: {:.0}% | {}",
            self.status_message,
            hours,
            minutes,
            seconds,
            millis,
            self.timeline_zoom,
            if self.is_playing { "Playing" } else { "Stopped" }
        ))
        .size(14);

        let top_row = row![source_panel, video_viewport].spacing(4);

        let base_layout: Element<'_, Message> = if self.open_menu.is_some() {
            let click_off: Element<'_, Message> = mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_press(Message::CloseMenu)
            .into();

            let dropdown = self.view_dropdown();

            let content_below = column![top_row, timeline_panel, status].spacing(4);

            let stacked_content = stack![content_below, click_off, opaque(dropdown)]
                .width(Length::Fill)
                .height(Length::Fill);

            column![menu_bar, stacked_content]
                .spacing(4)
                .padding(4)
                .into()
        } else {
            column![menu_bar, top_row, timeline_panel, status]
                .spacing(4)
                .padding(4)
                .into()
        };

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
        if let Some(drag) = &self.drag_state {
            let ghost = self.view_drag_overlay(drag);
            stack![base_layout, ghost]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            base_layout
        }
    }

    fn view_source_library(&self) -> Element<'_, Message> {
        let title = text("Source Library").size(18);

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

        column![title, import_btn, asset_grid]
            .spacing(8)
            .width(300)
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
                .content_fit(iced::ContentFit::Cover)
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
        .height(200);

        let timeline_content: Element<'_, Message> = column![controls, canvas].spacing(4).into();

        // When dragging, wrap timeline in mouse_area for enter/exit/move detection
        if self.drag_state.is_some() {
            mouse_area(timeline_content)
                .on_enter(Message::DragEnteredTimeline)
                .on_exit(Message::DragExitedTimeline)
                .on_move(Message::DragOverTimeline)
                .into()
        } else {
            timeline_content
        }
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
                .content_fit(iced::ContentFit::Cover)
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

        let clip_info =
            self.clip_at_position(self.playback_position)
                .map(|(_, clip)| {
                    (
                        clip.id,
                        clip.asset_id,
                        clip.timeline_range.start.as_secs_f64(),
                        clip.source_range.start.as_secs_f64(),
                    )
                });

        if let Some((clip_id, asset_id, clip_tl_start, clip_src_start)) = clip_info {
            let playback_pos = self.playback_position.as_secs_f64();
            let source_time = clip_src_start + (playback_pos - clip_tl_start);
            let path = self
                .project
                .source_library
                .get(asset_id)
                .map(|a| a.path.clone());

            if let Some(path) = path {
                self.decode_clip_id = Some(clip_id);
                // Store offset so we can convert source PTS → timeline time:
                // timeline_time = pts_secs + (clip_tl_start - clip_src_start)
                self.decode_time_offset = clip_tl_start - clip_src_start;
                if let Some(tx) = &self.decode_tx {
                    let _ = tx.send(DecodeRequest::Seek {
                        path,
                        time: source_time,
                        continuous,
                    });
                }
                return;
            }
        }
        self.decode_clip_id = None;
        self.current_frame = None;
        self.pending_frame = None;
        self.send_decode_stop();
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

        let clip_info =
            self.audio_clip_at_position(self.playback_position)
                .map(|(_, clip)| {
                    (
                        clip.id,
                        clip.asset_id,
                        clip.timeline_range.start.as_secs_f64(),
                        clip.source_range.start.as_secs_f64(),
                    )
                });

        if let Some((clip_id, asset_id, clip_tl_start, clip_src_start)) = clip_info {
            let playback_pos = self.playback_position.as_secs_f64();
            let source_time = clip_src_start + (playback_pos - clip_tl_start);
            let path = self
                .project
                .source_library
                .get(asset_id)
                .map(|a| a.path.clone());

            if let Some(path) = path {
                // Clear buffered audio from previous clip before starting new one.
                // Without this, adjacent clips (no gap) would keep playing clip1's
                // buffered audio instead of transitioning to clip2.
                if let Some(player) = &self.audio_player {
                    player.clear();
                    player.play();
                }
                self.audio_decode_clip_id = Some(clip_id);
                self.audio_decode_time_offset = clip_tl_start - clip_src_start;
                if let Some(tx) = &self.audio_decode_tx {
                    let _ = tx.send(AudioDecodeRequest::Seek {
                        path,
                        time: source_time,
                        continuous,
                    });
                }
                return;
            }
        }
        self.audio_decode_clip_id = None;
        self.send_audio_decode_stop();
        if let Some(player) = &self.audio_player {
            player.stop();
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
    path: PathBuf,
    decoder: zeditor_media::decoder::FfmpegDecoder,
    last_pts: f64,
}

/// Background decode worker thread. Owns the FFmpeg decoder and runs ahead of playback.
fn decode_worker(
    request_rx: mpsc::Receiver<DecodeRequest>,
    frame_tx: mpsc::SyncSender<DecodedFrame>,
) {
    use zeditor_media::decoder::{FfmpegDecoder, VideoDecoder};

    let mut decoder_state: Option<CachedDecoder> = None;
    let mut running = false;
    let mut is_continuous = false;
    let mut target_time: f64 = 0.0;
    let mut seeking_to_target = false;

    loop {
        // If not actively decoding, block until we get a request
        // If actively decoding, check for new requests non-blocking
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
                DecodeRequest::Seek {
                    path,
                    time,
                    continuous,
                } => {
                    let needs_open = match &decoder_state {
                        Some(cached) => cached.path != path,
                        None => true,
                    };
                    if needs_open {
                        match FfmpegDecoder::open(&path) {
                            Ok(decoder) => {
                                decoder_state = Some(CachedDecoder {
                                    path,
                                    decoder,
                                    last_pts: -1.0,
                                });
                            }
                            Err(_) => {
                                running = false;
                                continue;
                            }
                        }
                    }
                    let cached = decoder_state.as_mut().unwrap();

                    // Decide whether to seek or decode forward
                    let needs_seek = time < cached.last_pts
                        || (time - cached.last_pts) > 2.0
                        || cached.last_pts < 0.0;

                    if needs_seek {
                        if cached.decoder.seek_to(time).is_err() {
                            running = false;
                            continue;
                        }
                        cached.last_pts = -1.0;
                        seeking_to_target = true;
                    } else {
                        seeking_to_target = false;
                    }
                    target_time = time;
                    is_continuous = continuous;
                    running = true;
                }
                DecodeRequest::Stop => {
                    running = false;
                    continue;
                }
            }
        }

        if running {
            let cached = decoder_state.as_mut().unwrap();
            match cached
                .decoder
                .decode_next_frame_rgba_scaled(PREVIEW_MAX_WIDTH, PREVIEW_MAX_HEIGHT)
            {
                Ok(Some(frame)) => {
                    cached.last_pts = frame.pts_secs;

                    // After a seek, skip pre-target frames (between keyframe and target)
                    if seeking_to_target && frame.pts_secs < target_time - 0.05 {
                        continue;
                    }
                    seeking_to_target = false;

                    let decoded = DecodedFrame {
                        rgba: frame.data,
                        width: frame.width,
                        height: frame.height,
                        pts_secs: frame.pts_secs,
                    };
                    // Send to UI; blocks if buffer is full (backpressure)
                    if frame_tx.send(decoded).is_err() {
                        return; // UI dropped the receiver
                    }

                    // In scrub mode, stop after sending one target frame
                    if !is_continuous {
                        running = false;
                    }
                }
                Ok(None) => {
                    // EOF
                    running = false;
                }
                Err(_) => {
                    running = false;
                }
            }
        }
    }
}

/// Cached audio decoder state for the audio decode worker thread.
struct CachedAudioDecoder {
    path: PathBuf,
    decoder: zeditor_media::audio_decoder::FfmpegAudioDecoder,
    last_pts: f64,
}

/// Background audio decode worker thread. Mirrors the video decode worker pattern.
fn audio_decode_worker(
    request_rx: mpsc::Receiver<AudioDecodeRequest>,
    audio_tx: mpsc::SyncSender<DecodedAudio>,
) {
    use zeditor_media::audio_decoder::FfmpegAudioDecoder;

    let mut decoder_state: Option<CachedAudioDecoder> = None;
    let mut running = false;
    let mut is_continuous = false;
    let mut target_time: f64 = 0.0;
    let mut seeking_to_target = false;

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
                AudioDecodeRequest::Seek {
                    path,
                    time,
                    continuous,
                } => {
                    let needs_open = match &decoder_state {
                        Some(cached) => cached.path != path,
                        None => true,
                    };
                    if needs_open {
                        match FfmpegAudioDecoder::open(&path) {
                            Ok(decoder) => {
                                decoder_state = Some(CachedAudioDecoder {
                                    path,
                                    decoder,
                                    last_pts: -1.0,
                                });
                            }
                            Err(_) => {
                                running = false;
                                continue;
                            }
                        }
                    }
                    let cached = decoder_state.as_mut().unwrap();

                    let needs_seek = time < cached.last_pts
                        || (time - cached.last_pts) > 2.0
                        || cached.last_pts < 0.0;

                    if needs_seek {
                        if cached.decoder.seek_to(time).is_err() {
                            running = false;
                            continue;
                        }
                        cached.last_pts = -1.0;
                        seeking_to_target = true;
                    } else {
                        seeking_to_target = false;
                    }
                    target_time = time;
                    is_continuous = continuous;
                    running = true;
                }
                AudioDecodeRequest::Stop => {
                    running = false;
                    continue;
                }
            }
        }

        if running {
            let cached = decoder_state.as_mut().unwrap();
            match cached.decoder.decode_next_audio_frame() {
                Ok(Some(frame)) => {
                    cached.last_pts = frame.pts_secs;

                    // Skip pre-target frames after seek
                    if seeking_to_target && frame.pts_secs < target_time - 0.05 {
                        continue;
                    }
                    seeking_to_target = false;

                    let decoded = DecodedAudio {
                        samples: frame.samples,
                        sample_rate: frame.sample_rate,
                        channels: frame.channels,
                        pts_secs: frame.pts_secs,
                    };
                    if audio_tx.send(decoded).is_err() {
                        return;
                    }

                    if !is_continuous {
                        running = false;
                    }
                }
                Ok(None) => {
                    running = false;
                }
                Err(_) => {
                    running = false;
                }
            }
        }
    }
}
