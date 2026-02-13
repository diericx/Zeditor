use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use iced::widget::{button, center, column, container, row, scrollable, text};
use iced::{keyboard, time, Element, Length, Subscription, Task};
use uuid::Uuid;

use zeditor_core::project::Project;
use zeditor_core::timeline::{Clip, TimeRange, TimelinePosition};

use crate::message::Message;
use crate::widgets::timeline_canvas::TimelineCanvas;

pub struct App {
    pub project: Project,
    pub playback_position: TimelinePosition,
    pub is_playing: bool,
    pub status_message: String,
    pub selected_asset_id: Option<Uuid>,
    pub current_frame: Option<iced::widget::image::Handle>,
    pub decode_in_flight: bool,
    pub playback_start_wall: Option<Instant>,
    pub playback_start_pos: TimelinePosition,
    pub timeline_zoom: f32,
    pub timeline_scroll: f32,
}

impl Default for App {
    fn default() -> Self {
        Self {
            project: Project::new("Untitled"),
            playback_position: TimelinePosition::zero(),
            is_playing: false,
            status_message: String::new(),
            selected_asset_id: None,
            current_frame: None,
            decode_in_flight: false,
            playback_start_wall: None,
            playback_start_pos: TimelinePosition::zero(),
            timeline_zoom: 100.0,
            timeline_scroll: 0.0,
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn boot() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs: Vec<Subscription<Message>> =
            vec![keyboard::listen().map(Message::KeyboardEvent)];

        if self.is_playing {
            subs.push(time::every(Duration::from_millis(33)).map(|_| Message::PlaybackTick));
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
                        self.project.source_library.import(asset);
                    }
                    Err(e) => {
                        self.status_message = format!("Import failed: {e}");
                    }
                }
                Task::none()
            }
            Message::RemoveAsset(id) => {
                match self.project.source_library.remove(id) {
                    Ok(asset) => {
                        self.status_message = format!("Removed: {}", asset.name);
                    }
                    Err(e) => {
                        self.status_message = format!("Remove failed: {e}");
                    }
                }
                Task::none()
            }
            Message::SelectSourceAsset(id) => {
                self.selected_asset_id = id;
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
                    let clip = Clip::new(asset_id, position, source_range);
                    let result = self.project.command_history.execute(
                        &mut self.project.timeline,
                        "Add clip",
                        |tl| tl.add_clip(track_index, clip),
                    );
                    match result {
                        Ok(()) => {
                            self.status_message = "Clip added".into();
                        }
                        Err(e) => {
                            self.status_message = format!("Add clip failed: {e}");
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
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Move clip",
                    |tl| {
                        tl.move_clip(source_track, clip_id, dest_track, position)?;
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
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Cut clip",
                    |tl| tl.cut_at(track_index, position),
                );
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
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Resize clip",
                    |tl| tl.resize_clip(track_index, clip_id, new_end),
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
                self.playback_position = pos;
                self.spawn_frame_decode()
            }
            Message::PlaceSelectedClip {
                asset_id,
                track_index,
                position,
            } => {
                self.selected_asset_id = None;
                if let Some(asset) = self.project.source_library.get(asset_id) {
                    let source_range = TimeRange {
                        start: TimelinePosition::zero(),
                        end: TimelinePosition::from_secs_f64(asset.duration.as_secs_f64()),
                    };
                    let clip = Clip::new(asset_id, position, source_range);
                    let result = self.project.command_history.execute(
                        &mut self.project.timeline,
                        "Place clip",
                        |tl| tl.add_clip(track_index, clip),
                    );
                    match result {
                        Ok(()) => {
                            self.status_message = "Clip placed".into();
                        }
                        Err(e) => {
                            self.status_message = format!("Place failed: {e}");
                        }
                    }
                } else {
                    self.status_message = "Asset not found".into();
                }
                self.spawn_frame_decode()
            }
            Message::Play => {
                self.is_playing = true;
                self.playback_start_wall = Some(Instant::now());
                self.playback_start_pos = self.playback_position;
                Task::none()
            }
            Message::Pause => {
                self.is_playing = false;
                self.playback_start_wall = None;
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
                    let timeline_dur = self.project.timeline.duration().as_secs_f64();
                    if new_pos >= timeline_dur && timeline_dur > 0.0 {
                        self.playback_position =
                            TimelinePosition::from_secs_f64(timeline_dur);
                        self.is_playing = false;
                        self.playback_start_wall = None;
                        return Task::none();
                    }
                    self.playback_position = TimelinePosition::from_secs_f64(new_pos);

                    // Auto-scroll: keep playhead visible
                    let playhead_px =
                        new_pos as f32 * self.timeline_zoom - self.timeline_scroll;
                    let visible_width = 800.0; // approximate
                    if playhead_px > visible_width * 0.8 {
                        self.timeline_scroll =
                            new_pos as f32 * self.timeline_zoom - visible_width * 0.5;
                    }

                    return self.spawn_frame_decode();
                }
                Task::none()
            }
            Message::SeekTo(pos) => {
                self.playback_position = pos;
                if self.is_playing {
                    self.playback_start_wall = Some(Instant::now());
                    self.playback_start_pos = pos;
                }
                self.spawn_frame_decode()
            }
            Message::FrameDecoded(result) => {
                self.decode_in_flight = false;
                match result {
                    Ok((rgba_data, width, height)) => {
                        self.current_frame = Some(iced::widget::image::Handle::from_rgba(
                            width, height, rgba_data,
                        ));
                    }
                    Err(_e) => {
                        // Keep showing previous frame on decode error
                    }
                }
                Task::none()
            }
            Message::KeyboardEvent(event) => {
                if let keyboard::Event::KeyPressed { key, .. } = event {
                    if key == keyboard::Key::Named(keyboard::key::Named::Space) {
                        return self.update(Message::TogglePlayback);
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
            Message::SaveProject | Message::LoadProject(_) => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
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

        let content = column![top_row, timeline_panel, status]
            .spacing(4)
            .padding(4);

        container(content).into()
    }

    fn view_source_library(&self) -> Element<'_, Message> {
        let title = text("Source Library").size(18);

        let import_btn =
            button(text("Import").size(14)).on_press(Message::OpenFileDialog);

        let assets: Vec<Element<'_, Message>> = self
            .project
            .source_library
            .assets()
            .iter()
            .map(|asset| {
                let is_selected = self.selected_asset_id == Some(asset.id);
                let label = text(&asset.name).size(14);
                let select_btn = if is_selected {
                    button(text("Selected").size(12))
                        .on_press(Message::SelectSourceAsset(None))
                        .style(button::success)
                } else {
                    button(text("Select").size(12))
                        .on_press(Message::SelectSourceAsset(Some(asset.id)))
                };
                let add_btn = button(text("Add to Timeline").size(12)).on_press(
                    Message::AddClipToTimeline {
                        asset_id: asset.id,
                        track_index: 0,
                        position: self.project.timeline.track(0).map_or(
                            TimelinePosition::zero(),
                            |t| t.end_position(),
                        ),
                    },
                );
                row![label, select_btn, add_btn].spacing(5).into()
            })
            .collect();

        let asset_list = scrollable(column(assets).spacing(4));

        let add_to_tl_btn = if self.selected_asset_id.is_some() {
            text("Click timeline to place clip").size(12)
        } else {
            text("").size(12)
        };

        column![title, import_btn, asset_list, add_to_tl_btn]
            .spacing(8)
            .width(300)
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

        let video_area: Element<'_, Message> = if let Some(handle) = &self.current_frame {
            container(
                iced::widget::image(handle.clone())
                    .content_fit(iced::ContentFit::Contain)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(300)
            .into()
        } else {
            container(center(text("No video").size(16)))
                .width(Length::Fill)
                .height(300)
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

        let canvas = iced::widget::canvas(TimelineCanvas {
            timeline: &self.project.timeline,
            playback_position: self.playback_position,
            selected_asset_id: self.selected_asset_id,
            zoom: self.timeline_zoom,
            scroll_offset: self.timeline_scroll,
        })
        .width(Length::Fill)
        .height(200);

        column![controls, canvas].spacing(4).into()
    }

    /// Find the clip at the given playback position across all tracks.
    pub fn clip_at_position(&self, pos: TimelinePosition) -> Option<(usize, &Clip)> {
        for (i, track) in self.project.timeline.tracks.iter().enumerate() {
            if let Some(clip) = track.clip_at(pos) {
                return Some((i, clip));
            }
        }
        None
    }

    /// Spawn a background task to decode the frame at the current playback position.
    fn spawn_frame_decode(&mut self) -> Task<Message> {
        if self.decode_in_flight {
            return Task::none();
        }

        let clip_info = self.clip_at_position(self.playback_position);
        if let Some((_track_idx, clip)) = clip_info {
            let asset_id = clip.asset_id;
            let clip_tl_start = clip.timeline_range.start.as_secs_f64();
            let clip_src_start = clip.source_range.start.as_secs_f64();
            let playback_pos = self.playback_position.as_secs_f64();
            let source_time = clip_src_start + (playback_pos - clip_tl_start);

            if let Some(asset) = self.project.source_library.get(asset_id) {
                let path = asset.path.clone();
                self.decode_in_flight = true;
                return Task::perform(
                    async move { decode_frame_at(path, source_time) },
                    Message::FrameDecoded,
                );
            }
        }
        Task::none()
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

/// Preview resolution cap. 4K frames are scaled down to this for display.
const PREVIEW_MAX_WIDTH: u32 = 960;
const PREVIEW_MAX_HEIGHT: u32 = 540;

/// Cached decoder state. Keeps the decoder open between frames for the same file.
struct CachedDecoder {
    path: PathBuf,
    decoder: zeditor_media::decoder::FfmpegDecoder,
    last_pts: f64,
}

static DECODER_CACHE: Mutex<Option<CachedDecoder>> = Mutex::new(None);

/// Decode a single frame at the given source timestamp. Returns RGBA data + dimensions.
/// Reuses the decoder if the path matches, only seeking when necessary.
/// Scales down to preview resolution for performance.
fn decode_frame_at(path: PathBuf, source_time: f64) -> Result<(Vec<u8>, u32, u32), String> {
    use zeditor_media::decoder::{FfmpegDecoder, VideoDecoder};

    let mut cache = DECODER_CACHE.lock().map_err(|e| format!("lock: {e}"))?;

    // Reuse existing decoder if same path
    let needs_open = match &*cache {
        Some(cached) => cached.path != path,
        None => true,
    };

    if needs_open {
        let decoder = FfmpegDecoder::open(&path).map_err(|e| format!("{e}"))?;
        *cache = Some(CachedDecoder {
            path: path.clone(),
            decoder,
            last_pts: -1.0,
        });
    }

    let cached = cache.as_mut().unwrap();

    // Decide whether to seek:
    // - If target is before current position, must seek backward
    // - If target is far ahead (>2s), seek rather than decode through
    // - If target is close ahead, just decode forward (much faster)
    let needs_seek = source_time < cached.last_pts
        || (source_time - cached.last_pts) > 2.0
        || cached.last_pts < 0.0;

    if needs_seek {
        cached
            .decoder
            .seek_to(source_time)
            .map_err(|e| format!("{e}"))?;
        cached.last_pts = -1.0;
    }

    // Decode frames until we reach or pass the target timestamp.
    // Use scaled decode to avoid processing full 4K pixels.
    loop {
        match cached
            .decoder
            .decode_next_frame_scaled(PREVIEW_MAX_WIDTH, PREVIEW_MAX_HEIGHT)
            .map_err(|e| format!("{e}"))?
        {
            Some(frame) => {
                cached.last_pts = frame.pts_secs;
                // Accept this frame if it's at or past the target
                if frame.pts_secs >= source_time - 0.05 {
                    let rgba = rgb24_to_rgba32(&frame.data, frame.width, frame.height);
                    return Ok((rgba, frame.width, frame.height));
                }
                // Otherwise keep decoding forward (skipping pre-target frames)
            }
            None => {
                // Hit EOF - clear cache for next use
                *cache = None;
                return Err("No frame found".into());
            }
        }
    }
}
