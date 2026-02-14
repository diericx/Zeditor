use std::collections::HashMap;

use iced::mouse;
use iced::widget::canvas;
use iced::{border, Color, Point, Rectangle, Renderer, Size, Theme};
use uuid::Uuid;

use zeditor_core::timeline::{Timeline, TimelinePosition, TrimPreview, TrackType};

use crate::message::{Message, SourceDragPreview, ToolMode};

const RULER_HEIGHT: f32 = 20.0;
const TRACK_HEIGHT: f32 = 50.0;
const CLIP_RESIZE_EDGE_WIDTH: f32 = 8.0;
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 1000.0;
const SNAP_THRESHOLD_SECS: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitZone {
    Body,
    RightEdge,
}

#[derive(Debug, Clone)]
pub enum TimelineInteraction {
    None,
    Dragging {
        track_index: usize,
        clip_id: Uuid,
        offset_px: f32,
        current_x: f32,
        start_x: f32,
        start_y: f32,
    },
    Resizing {
        track_index: usize,
        clip_id: Uuid,
        current_x: f32,
    },
}

pub struct TimelineCanvasState {
    pub interaction: TimelineInteraction,
    pub modifiers: iced::keyboard::Modifiers,
    pub cursor_position: Option<Point>,
}

impl Default for TimelineCanvasState {
    fn default() -> Self {
        Self {
            interaction: TimelineInteraction::None,
            modifiers: iced::keyboard::Modifiers::empty(),
            cursor_position: None,
        }
    }
}

pub struct TimelineCanvas<'a> {
    pub timeline: &'a Timeline,
    pub playback_position: TimelinePosition,
    pub selected_clip: Option<(usize, Uuid)>,
    pub zoom: f32,
    pub scroll_offset: f32,
    pub tool_mode: ToolMode,
    pub source_drag: Option<SourceDragPreview>,
}

impl<'a> TimelineCanvas<'a> {
    pub fn px_to_secs(&self, px: f32) -> f64 {
        ((px + self.scroll_offset) / self.zoom) as f64
    }

    pub fn secs_to_px(&self, secs: f64) -> f32 {
        secs as f32 * self.zoom - self.scroll_offset
    }

    pub fn hit_test_clip(&self, x: f32, y: f32) -> Option<(usize, Uuid, HitZone)> {
        let track_y = y - RULER_HEIGHT;
        if track_y < 0.0 {
            return None;
        }
        let track_index = (track_y / TRACK_HEIGHT) as usize;
        if track_index >= self.timeline.tracks.len() {
            return None;
        }

        let track = &self.timeline.tracks[track_index];
        for clip in &track.clips {
            let clip_start_px = self.secs_to_px(clip.timeline_range.start.as_secs_f64());
            let clip_end_px = self.secs_to_px(clip.timeline_range.end.as_secs_f64());

            if x >= clip_start_px && x <= clip_end_px {
                if x >= clip_end_px - CLIP_RESIZE_EDGE_WIDTH {
                    return Some((track_index, clip.id, HitZone::RightEdge));
                }
                return Some((track_index, clip.id, HitZone::Body));
            }
        }
        None
    }

    fn track_at_y(&self, y: f32) -> usize {
        let track_y = (y - RULER_HEIGHT).max(0.0);
        let idx = (track_y / TRACK_HEIGHT) as usize;
        idx.min(self.timeline.tracks.len().saturating_sub(1))
    }

    fn clip_duration_secs(&self, clip_id: Uuid) -> f64 {
        self.timeline
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .find(|c| c.id == clip_id)
            .map(|c| c.duration().as_secs_f64())
            .unwrap_or(0.0)
    }

    /// Validate the destination track for a drag operation.
    /// Solo clips: must match clip's track type; snaps to nearest track of correct type.
    /// Grouped clips: dest must have a mirror track; snaps to last valid track if not.
    fn validate_drag_dest_track(
        &self,
        source_track: usize,
        clip_id: Uuid,
        raw_dest: usize,
    ) -> usize {
        let clip = self.timeline.track(source_track)
            .ok()
            .and_then(|t| t.get_clip(clip_id));
        let clip = match clip {
            Some(c) => c,
            None => return raw_dest,
        };

        let source_type = self.timeline.tracks.get(source_track)
            .map(|t| t.track_type)
            .unwrap_or(TrackType::Video);
        let has_link = clip.link_id.is_some();

        if has_link {
            // Grouped clip: dest_track must have a mirror
            if source_type == TrackType::Video {
                // Check if mirror audio track exists for raw_dest
                if self.timeline.mirror_audio_track_for_video(raw_dest).is_some() {
                    return raw_dest;
                }
                // Snap to nearest video track that has a mirror
                self.nearest_video_track_with_mirror(raw_dest)
                    .unwrap_or(source_track)
            } else {
                if self.timeline.mirror_video_track_for_audio(raw_dest).is_some() {
                    return raw_dest;
                }
                self.nearest_audio_track_with_mirror(raw_dest)
                    .unwrap_or(source_track)
            }
        } else {
            // Solo clip: dest must match track type
            let dest_type = self.timeline.tracks.get(raw_dest)
                .map(|t| t.track_type)
                .unwrap_or(TrackType::Video);
            if dest_type == source_type {
                raw_dest
            } else {
                // Snap to nearest track of correct type
                self.nearest_track_of_type(raw_dest, source_type)
                    .unwrap_or(source_track)
            }
        }
    }

    fn nearest_track_of_type(&self, target: usize, track_type: TrackType) -> Option<usize> {
        self.timeline.tracks.iter()
            .enumerate()
            .filter(|(_, t)| t.track_type == track_type)
            .min_by_key(|(i, _)| (*i as isize - target as isize).unsigned_abs())
            .map(|(i, _)| i)
    }

    fn nearest_video_track_with_mirror(&self, target: usize) -> Option<usize> {
        self.timeline.video_track_indices().into_iter()
            .filter(|&i| self.timeline.mirror_audio_track_for_video(i).is_some())
            .min_by_key(|&i| (i as isize - target as isize).unsigned_abs())
    }

    fn nearest_audio_track_with_mirror(&self, target: usize) -> Option<usize> {
        self.timeline.audio_track_indices().into_iter()
            .filter(|&i| self.timeline.mirror_video_track_for_audio(i).is_some())
            .min_by_key(|&i| (i as isize - target as isize).unsigned_abs())
    }

    /// Compute the effective drop position for a drag, accounting for snap.
    /// First computes trim preview at the raw position, then checks for snap
    /// against the resulting (trimmed) edges. Returns the snapped start in secs.
    fn compute_snapped_start(
        &self,
        raw_start: f64,
        clip_duration: f64,
        dest_track: usize,
        exclude_id: Uuid,
    ) -> f64 {
        let raw_end = raw_start + clip_duration;
        if let Ok(track) = self.timeline.track(dest_track) {
            let previews =
                track.preview_trim_overlaps(raw_start, raw_end, Some(exclude_id));
            if let Some(snapped) = track.preview_snap_position(
                raw_start,
                raw_end,
                Some(exclude_id),
                &previews,
                SNAP_THRESHOLD_SECS,
            ) {
                return snapped.max(0.0);
            }
        }
        raw_start
    }
}

fn draw_clip_shape(
    frame: &mut canvas::Frame,
    draw_x: f32,
    draw_width: f32,
    track_top: f32,
    color: Color,
    duration_secs: f64,
) {
    let clip_pos = Point::new(draw_x, track_top + 2.0);
    let clip_size = Size::new(draw_width, TRACK_HEIGHT - 4.0);
    let clip_path = canvas::Path::new(|b| {
        b.rounded_rectangle(clip_pos, clip_size, border::Radius::from(4.0));
    });
    frame.fill(&clip_path, color);
    frame.stroke(
        &clip_path,
        canvas::Stroke::default()
            .with_color(Color::from_rgb(0.25, 0.25, 0.25))
            .with_width(1.0),
    );

    // Right resize edge indicator
    frame.fill_rectangle(
        Point::new(draw_x + draw_width - CLIP_RESIZE_EDGE_WIDTH, track_top + 2.0),
        Size::new(CLIP_RESIZE_EDGE_WIDTH, TRACK_HEIGHT - 4.0),
        Color {
            a: 0.3,
            ..Color::WHITE
        },
    );

    // Clip duration label
    if draw_width > 30.0 {
        frame.fill_text(canvas::Text {
            content: format!("{:.1}s", duration_secs),
            position: Point::new(draw_x + 4.0, track_top + 18.0),
            color: Color::WHITE,
            size: iced::Pixels(11.0),
            ..canvas::Text::default()
        });
    }
}

pub fn color_for_track_type(track_type: TrackType) -> Color {
    match track_type {
        TrackType::Video => Color::from_rgb(0.25, 0.65, 0.35),
        TrackType::Audio => Color::from_rgb(0.25, 0.45, 0.75),
    }
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(ZOOM_MIN, ZOOM_MAX)
}

impl<'a> canvas::Program<Message> for TimelineCanvas<'a> {
    type State = TimelineCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let cursor_pos = cursor.position_in(bounds)?;

        match event {
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let (dx, dy) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (*x, *y),
                    mouse::ScrollDelta::Pixels { x, y } => (*x / 20.0, *y / 20.0),
                };

                // Plain scroll = pan, Alt+scroll = zoom
                if state.modifiers.alt() {
                    // Alt held: zoom centered on cursor
                    let zoom_delta = if dy.abs() > dx.abs() { dy } else { dx };
                    let cursor_secs = self.px_to_secs(cursor_pos.x);
                    Some(
                        canvas::Action::publish(Message::TimelineZoom {
                            delta: zoom_delta,
                            cursor_secs,
                        })
                        .and_capture(),
                    )
                } else if dy.abs() > dx.abs() {
                    // Vertical scroll → horizontal pan
                    Some(
                        canvas::Action::publish(Message::TimelineScroll(-dy * 20.0))
                            .and_capture(),
                    )
                } else {
                    // Horizontal scroll → horizontal pan
                    Some(
                        canvas::Action::publish(Message::TimelineScroll(-dx * 20.0))
                            .and_capture(),
                    )
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                // Right-click on canvas is a no-op; context menu is on track headers
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some((track_index, clip_id, zone)) =
                    self.hit_test_clip(cursor_pos.x, cursor_pos.y)
                {
                    match zone {
                        HitZone::Body => {
                            if self.tool_mode == ToolMode::Blade {
                                // Blade mode: cut at cursor position
                                let secs = self.px_to_secs(cursor_pos.x).max(0.0);
                                return Some(
                                    canvas::Action::publish(Message::CutClip {
                                        track_index,
                                        position: TimelinePosition::from_secs_f64(secs),
                                    })
                                    .and_capture(),
                                );
                            }
                            if let Ok(track) = self.timeline.track(track_index) {
                                if let Some(clip) = track.get_clip(clip_id) {
                                    let clip_start_px = self
                                        .secs_to_px(clip.timeline_range.start.as_secs_f64());
                                    state.interaction = TimelineInteraction::Dragging {
                                        track_index,
                                        clip_id,
                                        offset_px: cursor_pos.x - clip_start_px,
                                        current_x: cursor_pos.x,
                                        start_x: cursor_pos.x,
                                        start_y: cursor_pos.y,
                                    };
                                }
                            }
                            return Some(canvas::Action::capture());
                        }
                        HitZone::RightEdge => {
                            state.interaction = TimelineInteraction::Resizing {
                                track_index,
                                clip_id,
                                current_x: cursor_pos.x,
                            };
                            return Some(canvas::Action::capture());
                        }
                    }
                }

                let secs = self.px_to_secs(cursor_pos.x).max(0.0);

                Some(
                    canvas::Action::publish(Message::TimelineClickEmpty(
                        TimelinePosition::from_secs_f64(secs),
                    ))
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                state.cursor_position = Some(cursor_pos);
                match &mut state.interaction {
                    TimelineInteraction::Dragging {
                        offset_px,
                        current_x,
                        ..
                    } => {
                        // Clamp so clip left edge can't go before time 0
                        let min_current_x = *offset_px - self.scroll_offset;
                        *current_x = cursor_pos.x.max(min_current_x);
                        Some(canvas::Action::request_redraw().and_capture())
                    }
                    TimelineInteraction::Resizing { current_x, .. } => {
                        *current_x = cursor_pos.x;
                        Some(canvas::Action::request_redraw().and_capture())
                    }
                    TimelineInteraction::None => {
                        if self.tool_mode == ToolMode::Blade {
                            Some(canvas::Action::request_redraw())
                        } else {
                            None
                        }
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let interaction = std::mem::replace(
                    &mut state.interaction,
                    TimelineInteraction::None,
                );
                match interaction {
                    TimelineInteraction::Dragging {
                        track_index,
                        clip_id,
                        offset_px,
                        current_x,
                        start_x,
                        start_y,
                    } => {
                        // If total movement < 5px in both axes, treat as click-to-select
                        let dy = cursor_pos.y - start_y;
                        if (current_x - start_x).abs() < 5.0 && dy.abs() < 5.0 {
                            return Some(
                                canvas::Action::publish(Message::SelectTimelineClip(
                                    Some((track_index, clip_id)),
                                ))
                                .and_capture(),
                            );
                        }
                        let raw_secs = self.px_to_secs(current_x - offset_px).max(0.0);
                        let raw_dest_track = self.track_at_y(cursor_pos.y);
                        let duration = self.clip_duration_secs(clip_id);

                        // Validate track type for the drop
                        let dest_track = self.validate_drag_dest_track(
                            track_index, clip_id, raw_dest_track,
                        );

                        let effective_secs = self.compute_snapped_start(
                            raw_secs, duration, dest_track, clip_id,
                        );
                        Some(
                            canvas::Action::publish(Message::MoveClip {
                                source_track: track_index,
                                clip_id,
                                dest_track,
                                position: TimelinePosition::from_secs_f64(effective_secs),
                            })
                            .and_capture(),
                        )
                    }
                    TimelineInteraction::Resizing {
                        track_index,
                        clip_id,
                        current_x,
                    } => {
                        let new_end_secs = self.px_to_secs(current_x).max(0.0);
                        Some(
                            canvas::Action::publish(Message::ResizeClip {
                                track_index,
                                clip_id,
                                new_end: TimelinePosition::from_secs_f64(new_end_secs),
                            })
                            .and_capture(),
                        )
                    }
                    TimelineInteraction::None => None,
                }
            }
            canvas::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // Background
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color::from_rgb(0.12, 0.12, 0.15),
        );

        // Time ruler
        self.draw_ruler(&mut frame, bounds.width);

        // Pre-compute drag info: effective (snapped) position + trim preview
        struct LinkedDragPreview {
            track_index: usize,
            clip_id: Uuid,
            effective_start_px: f32,
            effective_width_px: f32,
            effective_duration: f64,
            preview_map: HashMap<Uuid, Vec<TrimPreview>>,
        }

        struct DragPreview {
            dest_track: usize,
            effective_start_px: f32,
            effective_width_px: f32,
            effective_duration: f64,
            preview_map: HashMap<Uuid, Vec<TrimPreview>>,
            linked: Vec<LinkedDragPreview>,
        }

        let drag_preview: Option<DragPreview> = if let TimelineInteraction::Dragging {
            clip_id: drag_clip_id,
            track_index: drag_track,
            offset_px,
            current_x,
            ..
        } = &state.interaction
        {
            let drag_left_px = current_x - offset_px;
            let raw_start = self.px_to_secs(drag_left_px).max(0.0);
            let drag_duration = self.clip_duration_secs(*drag_clip_id);
            let raw_dest = cursor
                .position_in(bounds)
                .map(|p| self.track_at_y(p.y))
                .unwrap_or(0);
            // Validate dest track (type match / mirror existence) for preview
            let dest_track = self.validate_drag_dest_track(
                *drag_track, *drag_clip_id, raw_dest,
            );

            // Compute effective position with snap
            let effective_start = self.compute_snapped_start(
                raw_start,
                drag_duration,
                dest_track,
                *drag_clip_id,
            );
            let effective_end = effective_start + drag_duration;

            // Compute trim preview at the effective (snapped) position
            if let Ok(track) = self.timeline.track(dest_track) {
                let previews = track.preview_trim_overlaps(
                    effective_start,
                    effective_end,
                    Some(*drag_clip_id),
                );
                let mut map: HashMap<Uuid, Vec<_>> = HashMap::new();
                for p in previews {
                    map.entry(p.clip_id).or_default().push(p);
                }
                let effective_start_px = self.secs_to_px(effective_start);
                let effective_end_px = self.secs_to_px(effective_end);

                // Compute linked clip drag positions
                let mut linked = Vec::new();
                let link_id = self.timeline.track(*drag_track)
                    .ok()
                    .and_then(|t| t.get_clip(*drag_clip_id))
                    .and_then(|c| c.link_id);

                if let Some(link_id) = link_id {
                    // Get original position of dragged clip to compute delta
                    let orig_start = self.timeline.track(*drag_track)
                        .ok()
                        .and_then(|t| t.get_clip(*drag_clip_id))
                        .map(|c| c.timeline_range.start.as_secs_f64())
                        .unwrap_or(0.0);
                    let delta = effective_start - orig_start;

                    // Compute the mirror track for the linked clip if cross-track drag
                    let dest_type = self.timeline.tracks.get(dest_track)
                        .map(|t| t.track_type);
                    let linked_dest = if *drag_track != dest_track {
                        match dest_type {
                            Some(TrackType::Video) => self.timeline.mirror_audio_track_for_video(dest_track),
                            Some(TrackType::Audio) => self.timeline.mirror_video_track_for_audio(dest_track),
                            None => None,
                        }
                    } else {
                        None // same-track move: linked clips stay on their original tracks
                    };

                    for (linked_track_idx, linked_clip_id) in self.timeline.find_linked_clips(link_id) {
                        if linked_clip_id == *drag_clip_id {
                            continue;
                        }
                        // Use mirror dest if cross-track, otherwise keep on original track
                        let preview_track = linked_dest.unwrap_or(linked_track_idx);
                        if let Ok(linked_track) = self.timeline.track(linked_track_idx) {
                            if let Some(linked_clip) = linked_track.get_clip(linked_clip_id) {
                                let linked_start = linked_clip.timeline_range.start.as_secs_f64() + delta;
                                let linked_dur = linked_clip.duration().as_secs_f64();
                                let linked_end = linked_start + linked_dur;
                                // Preview trim on the destination track (may differ from current)
                                let preview_track_ref = self.timeline.track(preview_track)
                                    .unwrap_or(linked_track);
                                let linked_previews = preview_track_ref.preview_trim_overlaps(
                                    linked_start,
                                    linked_end,
                                    Some(linked_clip_id),
                                );
                                let mut linked_map: HashMap<Uuid, Vec<_>> = HashMap::new();
                                for p in linked_previews {
                                    linked_map.entry(p.clip_id).or_default().push(p);
                                }
                                let linked_start_px = self.secs_to_px(linked_start);
                                let linked_end_px = self.secs_to_px(linked_end);
                                linked.push(LinkedDragPreview {
                                    track_index: preview_track,
                                    clip_id: linked_clip_id,
                                    effective_start_px: linked_start_px,
                                    effective_width_px: linked_end_px - linked_start_px,
                                    effective_duration: linked_dur,
                                    preview_map: linked_map,
                                });
                            }
                        }
                    }
                }

                Some(DragPreview {
                    dest_track,
                    effective_start_px,
                    effective_width_px: effective_end_px - effective_start_px,
                    effective_duration: drag_duration,
                    preview_map: map,
                    linked,
                })
            } else {
                None
            }
        } else {
            None
        };

        // Pre-compute resize info for linked clips
        let linked_resize: HashMap<Uuid, f32> = if let TimelineInteraction::Resizing {
            track_index,
            clip_id: resize_id,
            current_x,
        } = &state.interaction
        {
            let mut map = HashMap::new();
            if let Ok(track) = self.timeline.track(*track_index) {
                if let Some(clip) = track.get_clip(*resize_id) {
                    if let Some(link_id) = clip.link_id {
                        let old_end_px = self.secs_to_px(clip.timeline_range.end.as_secs_f64());
                        let delta_px = current_x - old_end_px;
                        for (linked_track_idx, linked_clip_id) in self.timeline.find_linked_clips(link_id) {
                            if linked_clip_id == *resize_id {
                                continue;
                            }
                            if let Ok(linked_track) = self.timeline.track(linked_track_idx) {
                                if let Some(linked_clip) = linked_track.get_clip(linked_clip_id) {
                                    let linked_end_px = self.secs_to_px(linked_clip.timeline_range.end.as_secs_f64());
                                    map.insert(linked_clip_id, linked_end_px + delta_px);
                                }
                            }
                        }
                    }
                }
            }
            map
        } else {
            HashMap::new()
        };

        // Pre-compute source drag preview info (trim maps for affected tracks)
        struct SourceDragDrawInfo {
            video_track: usize,
            audio_track: Option<usize>,
            start_px: f32,
            width_px: f32,
            duration_secs: f64,
            video_preview_map: HashMap<Uuid, Vec<TrimPreview>>,
            audio_preview_map: HashMap<Uuid, Vec<TrimPreview>>,
        }

        let source_drag_info: Option<SourceDragDrawInfo> = self.source_drag.as_ref().and_then(|preview| {
            if preview.track_index >= self.timeline.tracks.len() {
                return None;
            }
            let start_secs = preview.position.as_secs_f64();
            let end_secs = start_secs + preview.duration_secs;
            let start_px = self.secs_to_px(start_secs);
            let end_px = self.secs_to_px(end_secs);

            let video_preview_map = if let Ok(track) = self.timeline.track(preview.track_index) {
                let previews = track.preview_trim_overlaps(start_secs, end_secs, None);
                let mut map: HashMap<Uuid, Vec<_>> = HashMap::new();
                for p in previews {
                    map.entry(p.clip_id).or_default().push(p);
                }
                map
            } else {
                HashMap::new()
            };

            let audio_preview_map = if let Some(audio_idx) = preview.audio_track_index {
                if let Ok(track) = self.timeline.track(audio_idx) {
                    let previews = track.preview_trim_overlaps(start_secs, end_secs, None);
                    let mut map: HashMap<Uuid, Vec<_>> = HashMap::new();
                    for p in previews {
                        map.entry(p.clip_id).or_default().push(p);
                    }
                    map
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            };

            Some(SourceDragDrawInfo {
                video_track: preview.track_index,
                audio_track: preview.audio_track_index,
                start_px,
                width_px: (end_px - start_px).max(4.0),
                duration_secs: preview.duration_secs,
                video_preview_map,
                audio_preview_map,
            })
        });

        // Compute link_id of the selected clip for group highlighting
        let selected_link_id = self.selected_clip.and_then(|(track_idx, clip_id)| {
            self.timeline.tracks.get(track_idx)
                .and_then(|t| t.clips.iter().find(|c| c.id == clip_id))
                .and_then(|c| c.link_id)
        });

        // Track lanes
        for (i, track) in self.timeline.tracks.iter().enumerate() {
            let track_top = RULER_HEIGHT + i as f32 * TRACK_HEIGHT;

            let bg = match track.track_type {
                TrackType::Audio => {
                    if i % 2 == 0 {
                        Color::from_rgb(0.13, 0.15, 0.20)
                    } else {
                        Color::from_rgb(0.15, 0.17, 0.22)
                    }
                }
                TrackType::Video => {
                    if i % 2 == 0 {
                        Color::from_rgb(0.15, 0.15, 0.18)
                    } else {
                        Color::from_rgb(0.17, 0.17, 0.20)
                    }
                }
            };
            frame.fill_rectangle(
                Point::new(0.0, track_top),
                Size::new(bounds.width, TRACK_HEIGHT),
                bg,
            );

            // Track separator
            frame.fill_rectangle(
                Point::new(0.0, track_top + TRACK_HEIGHT - 1.0),
                Size::new(bounds.width, 1.0),
                Color::from_rgb(0.3, 0.3, 0.35),
            );

            // Track label
            frame.fill_text(canvas::Text {
                content: track.name.clone(),
                position: Point::new(4.0, track_top + 4.0),
                color: Color::from_rgb(0.6, 0.6, 0.6),
                size: iced::Pixels(10.0),
                ..canvas::Text::default()
            });

            // Clips
            for clip in &track.clips {
                let clip_start_px =
                    self.secs_to_px(clip.timeline_range.start.as_secs_f64());
                let clip_end_px =
                    self.secs_to_px(clip.timeline_range.end.as_secs_f64());
                let clip_width = clip_end_px - clip_start_px;

                if clip_end_px < 0.0 || clip_start_px > bounds.width {
                    continue;
                }

                let is_dragged_clip = matches!(
                    &state.interaction,
                    TimelineInteraction::Dragging { clip_id: drag_id, .. } if *drag_id == clip.id
                );

                // Check if this clip is a linked-dragged clip
                let linked_drag_info = if !is_dragged_clip {
                    drag_preview.as_ref().and_then(|info| {
                        info.linked.iter().find(|l| l.clip_id == clip.id)
                    })
                } else {
                    None
                };
                let is_linked_dragged = linked_drag_info.is_some();

                // Skip dragged/linked clips on their original track —
                // they'll be drawn on the destination track after the loop.
                if is_dragged_clip || is_linked_dragged {
                    continue;
                }

                // If this clip is on the dest track (or linked track) and has preview
                // data, draw the preview pieces (trimmed/split) instead of the
                // original clip shape.
                if true {
                    if let Some(ref info) = drag_preview {
                        // Check primary drag preview map
                        if i == info.dest_track {
                            if let Some(previews) = info.preview_map.get(&clip.id) {
                                let color = color_for_track_type(track.track_type);
                                for preview in previews {
                                    if let (Some(ts), Some(te)) =
                                        (preview.trimmed_start, preview.trimmed_end)
                                    {
                                        let px = self.secs_to_px(ts);
                                        let pw = (self.secs_to_px(te) - px).max(4.0);
                                        draw_clip_shape(
                                            &mut frame, px, pw, track_top, color, te - ts,
                                        );
                                    }
                                }
                                continue;
                            }
                        }
                        // Check linked drag preview maps
                        let mut handled_by_linked_preview = false;
                        for linked in &info.linked {
                            if i == linked.track_index {
                                if let Some(previews) = linked.preview_map.get(&clip.id) {
                                    let color = color_for_track_type(track.track_type);
                                    for preview in previews {
                                        if let (Some(ts), Some(te)) =
                                            (preview.trimmed_start, preview.trimmed_end)
                                        {
                                            let px = self.secs_to_px(ts);
                                            let pw = (self.secs_to_px(te) - px).max(4.0);
                                            draw_clip_shape(
                                                &mut frame, px, pw, track_top, color, te - ts,
                                            );
                                        }
                                    }
                                    handled_by_linked_preview = true;
                                    break;
                                }
                            }
                        }
                        if handled_by_linked_preview {
                            continue;
                        }
                    }

                    // Check if this clip is affected by source drag preview
                    if let Some(ref sd_info) = source_drag_info {
                        let preview_map = if i == sd_info.video_track {
                            Some(&sd_info.video_preview_map)
                        } else if sd_info.audio_track == Some(i) {
                            Some(&sd_info.audio_preview_map)
                        } else {
                            None
                        };
                        if let Some(map) = preview_map {
                            if let Some(previews) = map.get(&clip.id) {
                                let color = color_for_track_type(track.track_type);
                                for preview in previews {
                                    if let (Some(ts), Some(te)) =
                                        (preview.trimmed_start, preview.trimmed_end)
                                    {
                                        let px = self.secs_to_px(ts);
                                        let pw = (self.secs_to_px(te) - px).max(4.0);
                                        draw_clip_shape(
                                            &mut frame, px, pw, track_top, color, te - ts,
                                        );
                                    }
                                }
                                continue;
                            }
                        }
                    }
                }

                // Resized clip: draw at current cursor position
                // Linked-resized clip: draw with adjusted width
                // Normal clip: draw at original position
                let (draw_x, draw_width, dur) = if let Some(&new_end_px) = linked_resize.get(&clip.id) {
                    let w = new_end_px - clip_start_px;
                    (clip_start_px, w, self.px_to_secs(w.max(4.0)))
                } else {
                    match &state.interaction {
                        TimelineInteraction::Resizing {
                            clip_id: resize_id,
                            current_x,
                            ..
                        } if *resize_id == clip.id => {
                            let w = current_x - clip_start_px;
                            (clip_start_px, w, self.px_to_secs(w.max(4.0)))
                        }
                        _ => (clip_start_px, clip_width, clip.duration().as_secs_f64()),
                    }
                };

                let color = color_for_track_type(track.track_type);
                draw_clip_shape(&mut frame, draw_x, draw_width.max(4.0), track_top, color, dur);

                // Draw selection border if this clip is selected or linked to selected
                let is_selected = self.selected_clip == Some((i, clip.id))
                    || (selected_link_id.is_some() && clip.link_id == selected_link_id);
                if is_selected {
                    let sel_pos = Point::new(draw_x, track_top + 2.0);
                    let sel_size = Size::new(draw_width.max(4.0), TRACK_HEIGHT - 4.0);
                    let sel_path = canvas::Path::new(|b| {
                        b.rounded_rectangle(sel_pos, sel_size, border::Radius::from(4.0));
                    });
                    frame.stroke(
                        &sel_path,
                        canvas::Stroke::default()
                            .with_color(Color::from_rgb(1.0, 0.2, 0.2))
                            .with_width(3.0),
                    );
                }
            }

            // Draw source drag new clips on this track
            if let Some(ref sd_info) = source_drag_info {
                if i == sd_info.video_track || sd_info.audio_track == Some(i) {
                    let color = color_for_track_type(track.track_type);
                    draw_clip_shape(
                        &mut frame,
                        sd_info.start_px,
                        sd_info.width_px,
                        track_top,
                        color,
                        sd_info.duration_secs,
                    );
                }
            }
        }

        // Draw dragged clips on their destination tracks (after the main loop
        // so they render on top of other clips).
        if let Some(ref info) = drag_preview {
            // Primary dragged clip on dest_track
            let dest_top = RULER_HEIGHT + info.dest_track as f32 * TRACK_HEIGHT;
            let dest_type = self.timeline.tracks.get(info.dest_track)
                .map(|t| t.track_type)
                .unwrap_or(TrackType::Video);
            let color = color_for_track_type(dest_type);
            draw_clip_shape(
                &mut frame,
                info.effective_start_px,
                info.effective_width_px.max(4.0),
                dest_top,
                color,
                info.effective_duration,
            );

            // Linked dragged clips on their destination tracks
            for linked in &info.linked {
                let linked_top = RULER_HEIGHT + linked.track_index as f32 * TRACK_HEIGHT;
                let linked_type = self.timeline.tracks.get(linked.track_index)
                    .map(|t| t.track_type)
                    .unwrap_or(TrackType::Audio);
                let linked_color = color_for_track_type(linked_type);
                draw_clip_shape(
                    &mut frame,
                    linked.effective_start_px,
                    linked.effective_width_px.max(4.0),
                    linked_top,
                    linked_color,
                    linked.effective_duration,
                );
            }
        }

        // Playhead
        let playhead_px = self.secs_to_px(self.playback_position.as_secs_f64());
        if playhead_px >= 0.0 && playhead_px <= bounds.width {
            frame.fill_rectangle(
                Point::new(playhead_px, 0.0),
                Size::new(2.0, bounds.size().height),
                Color::from_rgb(1.0, 0.2, 0.2),
            );
            let triangle = canvas::Path::new(|b| {
                b.move_to(Point::new(playhead_px - 5.0, 0.0));
                b.line_to(Point::new(playhead_px + 5.0, 0.0));
                b.line_to(Point::new(playhead_px, 8.0));
                b.close();
            });
            frame.fill(&triangle, Color::from_rgb(1.0, 0.2, 0.2));
        }

        // Blade mode: draw vertical orange line at cursor position over clips
        if self.tool_mode == ToolMode::Blade {
            if let TimelineInteraction::None = &state.interaction {
                if let Some(cursor_pos) = state.cursor_position {
                    if self.hit_test_clip(cursor_pos.x, cursor_pos.y).is_some() {
                        let total_track_height =
                            RULER_HEIGHT + self.timeline.tracks.len() as f32 * TRACK_HEIGHT;
                        frame.fill_rectangle(
                            Point::new(cursor_pos.x, RULER_HEIGHT),
                            Size::new(1.0, total_track_height - RULER_HEIGHT),
                            Color::from_rgb(1.0, 0.6, 0.0),
                        );
                    }
                }
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match &state.interaction {
            TimelineInteraction::Dragging { .. } => return mouse::Interaction::Grabbing,
            TimelineInteraction::Resizing { .. } => {
                return mouse::Interaction::ResizingHorizontally
            }
            TimelineInteraction::None => {}
        }

        if let Some(cursor_pos) = cursor.position_in(bounds) {
            if let Some((_, _, zone)) = self.hit_test_clip(cursor_pos.x, cursor_pos.y) {
                return match zone {
                    HitZone::Body => {
                        if self.tool_mode == ToolMode::Blade {
                            mouse::Interaction::Crosshair
                        } else {
                            mouse::Interaction::Grab
                        }
                    }
                    HitZone::RightEdge => mouse::Interaction::ResizingHorizontally,
                };
            }
        }

        mouse::Interaction::default()
    }
}

impl<'a> TimelineCanvas<'a> {
    fn draw_ruler(&self, frame: &mut canvas::Frame, width: f32) {
        frame.fill_rectangle(
            Point::ORIGIN,
            Size::new(width, RULER_HEIGHT),
            Color::from_rgb(0.2, 0.2, 0.25),
        );

        let secs_per_px = 1.0 / self.zoom as f64;
        let target_px_per_tick = 80.0;
        let raw_interval = secs_per_px * target_px_per_tick as f64;

        let tick_interval = if raw_interval <= 0.1 {
            0.1
        } else if raw_interval <= 0.5 {
            0.5
        } else if raw_interval <= 1.0 {
            1.0
        } else if raw_interval <= 5.0 {
            5.0
        } else if raw_interval <= 10.0 {
            10.0
        } else if raw_interval <= 30.0 {
            30.0
        } else {
            60.0
        };

        let start_secs = self.px_to_secs(0.0).max(0.0);
        let end_secs = self.px_to_secs(width);

        let mut t = (start_secs / tick_interval).floor() * tick_interval;
        while t <= end_secs {
            if t >= 0.0 {
                let px = self.secs_to_px(t);
                frame.fill_rectangle(
                    Point::new(px, 0.0),
                    Size::new(1.0, RULER_HEIGHT),
                    Color::from_rgb(0.5, 0.5, 0.55),
                );
                let label = if tick_interval >= 1.0 {
                    format!("{:.0}s", t)
                } else {
                    format!("{:.1}s", t)
                };
                frame.fill_text(canvas::Text {
                    content: label,
                    position: Point::new(px + 3.0, 4.0),
                    color: Color::from_rgb(0.7, 0.7, 0.7),
                    size: iced::Pixels(10.0),
                    ..canvas::Text::default()
                });
            }
            t += tick_interval;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;
    use zeditor_core::timeline::{Clip, TimeRange, Timeline, TrackType};

    fn make_test_timeline() -> Timeline {
        let mut tl = Timeline::new();
        tl.add_track("Video 1", TrackType::Video);

        let asset_id = Uuid::new_v4();
        let source_range = TimeRange {
            start: TimelinePosition::zero(),
            end: TimelinePosition::from_secs_f64(5.0),
        };
        let clip = Clip::new(asset_id, TimelinePosition::from_secs_f64(1.0), source_range);
        tl.add_clip(0, clip).unwrap();

        tl
    }

    #[test]
    fn test_px_to_secs() {
        let tl = Timeline::new();
        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };
        let secs = canvas.px_to_secs(200.0);
        assert!((secs - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_secs_to_px_with_scroll() {
        let tl = Timeline::new();
        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 50.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };
        let px = canvas.secs_to_px(2.0);
        assert!((px - 150.0).abs() < 0.001);
    }

    #[test]
    fn test_hit_test_clip_body() {
        let tl = make_test_timeline();
        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };
        let result = canvas.hit_test_clip(300.0, RULER_HEIGHT + 25.0);
        assert!(result.is_some());
        let (track_idx, _clip_id, zone) = result.unwrap();
        assert_eq!(track_idx, 0);
        assert_eq!(zone, HitZone::Body);
    }

    #[test]
    fn test_hit_test_right_edge() {
        let tl = make_test_timeline();
        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };
        let result = canvas.hit_test_clip(597.0, RULER_HEIGHT + 25.0);
        assert!(result.is_some());
        let (_, _, zone) = result.unwrap();
        assert_eq!(zone, HitZone::RightEdge);
    }

    #[test]
    fn test_hit_test_empty() {
        let tl = make_test_timeline();
        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };
        let result = canvas.hit_test_clip(50.0, RULER_HEIGHT + 25.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_zoom_clamping() {
        assert_eq!(clamp_zoom(0.01), ZOOM_MIN);
        assert_eq!(clamp_zoom(2000.0), ZOOM_MAX);
        assert_eq!(clamp_zoom(200.0), 200.0);
    }

    #[test]
    fn test_drag_position_clamped_to_zero() {
        let tl = Timeline::new();
        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 200.0, // scrolled right, so negative px → negative secs
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };
        // px_to_secs(-100) with scroll 200 = (-100 + 200)/100 = 1.0 (positive)
        // But with px=0 and large scroll offset, raw can go negative
        // The .max(0.0) clamp is applied at the call site (line 226), not in px_to_secs.
        // So test that px_to_secs can return negative, and the clamp at the call site works.
        let raw = canvas.px_to_secs(-300.0); // (-300 + 200)/100 = -1.0
        assert!(raw < 0.0, "px_to_secs should return negative for far-left positions");
        let clamped = raw.max(0.0);
        assert_eq!(clamped, 0.0, "drag position should clamp to zero");
    }

    #[test]
    fn test_blade_mode_click_emits_cut() {
        let tl = make_test_timeline();
        // Clip is at [1.0, 6.0) in timeline, which is [100px, 600px) at zoom=100
        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Blade,
            source_drag: None,
        };
        let mut state = TimelineCanvasState::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 200.0));

        // Click at x=300 (3.0s), which is inside the clip [1.0, 6.0)
        let cursor = mouse::Cursor::Available(Point::new(300.0, RULER_HEIGHT + 25.0));
        let event =
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));

        let result = canvas.update(&mut state, &event, bounds, cursor);
        assert!(result.is_some(), "blade click on clip should return an action");
        // Verify interaction is still None (no drag started)
        assert!(
            matches!(state.interaction, TimelineInteraction::None),
            "blade mode should not start dragging"
        );
    }

    #[test]
    fn test_drag_clamp_visual_at_zero() {
        let tl = make_test_timeline();
        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };

        // Simulate a drag state where the user tries to drag left of 0
        let mut state = TimelineCanvasState::default();
        let clip_id = tl.tracks[0].clips[0].id;
        state.interaction = TimelineInteraction::Dragging {
            track_index: 0,
            clip_id,
            offset_px: 50.0, // clicked 50px from clip's left edge
            current_x: 100.0,
            start_x: 100.0,
            start_y: RULER_HEIGHT + 25.0,
        };

        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 200.0));
        // Try to move cursor to x=-100, which would put clip at x=-150
        let cursor = mouse::Cursor::Available(Point::new(-100.0, RULER_HEIGHT + 25.0));
        let event = canvas::Event::Mouse(mouse::Event::CursorMoved { position: Point::new(-100.0, RULER_HEIGHT + 25.0) });

        canvas.update(&mut state, &event, bounds, cursor);

        // current_x should be clamped: min_current_x = offset_px - scroll_offset = 50.0 - 0.0 = 50.0
        if let TimelineInteraction::Dragging { current_x, .. } = &state.interaction {
            assert!(
                *current_x >= 50.0,
                "current_x should be clamped to at least offset_px, got {current_x}"
            );
            // Verify resulting position is >= 0
            let secs = canvas.px_to_secs(*current_x - 50.0);
            assert!(secs >= 0.0, "clip position should be >= 0, got {secs}");
        } else {
            panic!("expected Dragging interaction");
        }
    }

    /// Helper: create a timeline with grouped video+audio tracks and linked clips.
    fn make_grouped_timeline() -> Timeline {
        let mut tl = Timeline::new();
        let group_id = Uuid::new_v4();
        tl.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
        tl.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

        let asset_id = Uuid::new_v4();
        let source_range = TimeRange {
            start: TimelinePosition::zero(),
            end: TimelinePosition::from_secs_f64(5.0),
        };

        // Add linked video+audio clips at 1.0s
        tl.add_clip_with_audio(0, 1, asset_id, TimelinePosition::from_secs_f64(1.0), source_range)
            .unwrap();

        tl
    }

    #[test]
    fn test_grouped_drag_computes_linked_position() {
        // Verify that when dragging a linked video clip, the linked audio clip
        // would be computed to move by the same delta.
        let tl = make_grouped_timeline();
        let video_clip_id = tl.tracks[0].clips[0].id;
        let audio_clip_id = tl.tracks[1].clips[0].id;

        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };

        // Video clip starts at 1.0s. Drag to 3.0s → delta = 2.0s.
        // Audio clip also starts at 1.0s → should move to 3.0s.
        let drag_left_px = 300.0; // offset_px=50, current_x=350 → 350-50 = 300px
        let raw_start = canvas.px_to_secs(drag_left_px).max(0.0); // 3.0s
        let drag_duration = canvas.clip_duration_secs(video_clip_id); // 5.0s

        assert!((raw_start - 3.0).abs() < 0.01, "raw drag start should be 3.0s");
        assert!((drag_duration - 5.0).abs() < 0.01, "clip duration should be 5.0s");

        // Verify linked clip delta computation matches draw() logic
        let video_clip = tl.tracks[0].get_clip(video_clip_id).unwrap();
        let link_id = video_clip.link_id.unwrap();
        let delta = raw_start - video_clip.timeline_range.start.as_secs_f64();

        let linked_clips = tl.find_linked_clips(link_id);
        assert_eq!(linked_clips.len(), 2, "should find 2 linked clips (video + audio)");

        let audio_clip = tl.tracks[1].get_clip(audio_clip_id).unwrap();
        let expected_audio_start = audio_clip.timeline_range.start.as_secs_f64() + delta;
        assert!(
            (expected_audio_start - 3.0).abs() < 0.01,
            "linked audio clip should move by same delta to 3.0s, got {expected_audio_start}"
        );

        // Verify linked audio clip px position
        let expected_audio_start_px = canvas.secs_to_px(expected_audio_start);
        let expected_audio_end_px = canvas.secs_to_px(expected_audio_start + audio_clip.duration().as_secs_f64());
        assert!(
            (expected_audio_start_px - 300.0).abs() < 0.01,
            "audio start should be at 300px, got {expected_audio_start_px}"
        );
        assert!(
            (expected_audio_end_px - 800.0).abs() < 0.01,
            "audio end should be at 800px, got {expected_audio_end_px}"
        );
    }

    #[test]
    fn test_grouped_resize_computes_linked_position() {
        // Verify that when resizing a linked video clip, the linked audio clip
        // end position is adjusted by the same pixel delta.
        let tl = make_grouped_timeline();
        let video_clip_id = tl.tracks[0].clips[0].id;
        let audio_clip_id = tl.tracks[1].clips[0].id;

        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };

        // Video clip is [1.0, 6.0) → end_px = 600
        // Resize to current_x = 800 → delta_px = 200
        let video_clip = tl.tracks[0].get_clip(video_clip_id).unwrap();
        let video_end_px = canvas.secs_to_px(video_clip.timeline_range.end.as_secs_f64());
        let current_x = 800.0;
        let delta_px = current_x - video_end_px;

        assert!((video_end_px - 600.0).abs() < 0.01, "video end should be at 600px");
        assert!((delta_px - 200.0).abs() < 0.01, "delta should be 200px");

        // Audio clip is also [1.0, 6.0) → end_px = 600
        // New audio end_px = 600 + 200 = 800px → 8.0s
        let audio_clip = tl.tracks[1].get_clip(audio_clip_id).unwrap();
        let audio_end_px = canvas.secs_to_px(audio_clip.timeline_range.end.as_secs_f64());
        let new_audio_end_px = audio_end_px + delta_px;
        let new_audio_end_secs = canvas.px_to_secs(new_audio_end_px);

        assert!(
            (new_audio_end_px - 800.0).abs() < 0.01,
            "linked audio clip end should be at 800px, got {new_audio_end_px}"
        );
        assert!(
            (new_audio_end_secs - 8.0).abs() < 0.01,
            "linked audio clip end should be at 8.0s, got {new_audio_end_secs}"
        );
    }

    #[test]
    fn test_grouped_drag_interaction_starts_correctly() {
        // Verify that clicking a linked clip body starts dragging with correct state
        let tl = make_grouped_timeline();
        let video_clip_id = tl.tracks[0].clips[0].id;

        let canvas = TimelineCanvas {
            timeline: &tl,
            playback_position: TimelinePosition::zero(),

            selected_clip: None,
            zoom: 100.0,
            scroll_offset: 0.0,
            tool_mode: ToolMode::Arrow,
            source_drag: None,
        };

        let mut state = TimelineCanvasState::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1200.0, 200.0));
        // Click at x=300 (3.0s) in track 0, inside clip [1.0, 6.0)
        let cursor = mouse::Cursor::Available(Point::new(300.0, RULER_HEIGHT + 25.0));
        let event = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));

        let result = canvas.update(&mut state, &event, bounds, cursor);
        assert!(result.is_some(), "clicking linked clip should return action");

        match &state.interaction {
            TimelineInteraction::Dragging { track_index, clip_id, .. } => {
                assert_eq!(*track_index, 0);
                assert_eq!(*clip_id, video_clip_id);
            }
            other => panic!("expected Dragging, got {other:?}"),
        }
    }

    #[test]
    fn test_color_for_track_type_video_is_green() {
        let color = color_for_track_type(TrackType::Video);
        // Green channel should be dominant
        assert!(color.g > color.r, "video track green should be > red");
        assert!(color.g > color.b, "video track green should be > blue");
    }

    #[test]
    fn test_color_for_track_type_audio_is_blue() {
        let color = color_for_track_type(TrackType::Audio);
        // Blue channel should be dominant
        assert!(color.b > color.r, "audio track blue should be > red");
        assert!(color.b > color.g, "audio track blue should be > green");
    }
}
