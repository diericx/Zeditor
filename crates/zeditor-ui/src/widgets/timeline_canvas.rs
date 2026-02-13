use iced::mouse;
use iced::widget::canvas;
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use uuid::Uuid;

use zeditor_core::timeline::{Timeline, TimelinePosition};

use crate::message::Message;

const RULER_HEIGHT: f32 = 20.0;
const TRACK_HEIGHT: f32 = 50.0;
const CLIP_RESIZE_EDGE_WIDTH: f32 = 8.0;
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 1000.0;

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
    },
    Resizing {
        track_index: usize,
        clip_id: Uuid,
        current_x: f32,
    },
}

pub struct TimelineCanvasState {
    pub interaction: TimelineInteraction,
}

impl Default for TimelineCanvasState {
    fn default() -> Self {
        Self {
            interaction: TimelineInteraction::None,
        }
    }
}

pub struct TimelineCanvas<'a> {
    pub timeline: &'a Timeline,
    pub playback_position: TimelinePosition,
    pub selected_asset_id: Option<Uuid>,
    pub zoom: f32,
    pub scroll_offset: f32,
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
}

fn color_from_uuid(id: Uuid) -> Color {
    let bytes = id.as_bytes();
    let r = bytes[0] as f32 / 255.0 * 0.6 + 0.3;
    let g = bytes[4] as f32 / 255.0 * 0.6 + 0.3;
    let b = bytes[8] as f32 / 255.0 * 0.6 + 0.3;
    Color::from_rgb(r, g, b)
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

                // Vertical scroll = zoom, horizontal scroll = pan
                // Most mice: vertical only. Trackpads send both.
                if dy.abs() > dx.abs() {
                    let cursor_secs = self.px_to_secs(cursor_pos.x);
                    Some(
                        canvas::Action::publish(Message::TimelineZoom {
                            delta: dy,
                            cursor_secs,
                        })
                        .and_capture(),
                    )
                } else {
                    // Horizontal scroll for panning
                    Some(
                        canvas::Action::publish(Message::TimelineScroll(-dx * 20.0))
                            .and_capture(),
                    )
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some((track_index, clip_id, zone)) =
                    self.hit_test_clip(cursor_pos.x, cursor_pos.y)
                {
                    match zone {
                        HitZone::Body => {
                            if let Ok(track) = self.timeline.track(track_index) {
                                if let Some(clip) = track.get_clip(clip_id) {
                                    let clip_start_px = self
                                        .secs_to_px(clip.timeline_range.start.as_secs_f64());
                                    state.interaction = TimelineInteraction::Dragging {
                                        track_index,
                                        clip_id,
                                        offset_px: cursor_pos.x - clip_start_px,
                                        current_x: cursor_pos.x,
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
                let track_index = self.track_at_y(cursor_pos.y);

                if let Some(asset_id) = self.selected_asset_id {
                    return Some(
                        canvas::Action::publish(Message::PlaceSelectedClip {
                            asset_id,
                            track_index,
                            position: TimelinePosition::from_secs_f64(secs),
                        })
                        .and_capture(),
                    );
                }

                Some(
                    canvas::Action::publish(Message::TimelineClickEmpty(
                        TimelinePosition::from_secs_f64(secs),
                    ))
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                match &mut state.interaction {
                    TimelineInteraction::Dragging { current_x, .. } => {
                        *current_x = cursor_pos.x;
                        Some(canvas::Action::request_redraw().and_capture())
                    }
                    TimelineInteraction::Resizing { current_x, .. } => {
                        *current_x = cursor_pos.x;
                        Some(canvas::Action::request_redraw().and_capture())
                    }
                    TimelineInteraction::None => None,
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
                    } => {
                        let new_secs = self.px_to_secs(current_x - offset_px).max(0.0);
                        let dest_track = self.track_at_y(cursor_pos.y);
                        Some(
                            canvas::Action::publish(Message::MoveClip {
                                source_track: track_index,
                                clip_id,
                                dest_track,
                                position: TimelinePosition::from_secs_f64(new_secs),
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
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
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

        // Track lanes
        for (i, track) in self.timeline.tracks.iter().enumerate() {
            let track_top = RULER_HEIGHT + i as f32 * TRACK_HEIGHT;

            let bg = if i % 2 == 0 {
                Color::from_rgb(0.15, 0.15, 0.18)
            } else {
                Color::from_rgb(0.17, 0.17, 0.20)
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

                let (draw_x, draw_width) = match &state.interaction {
                    TimelineInteraction::Dragging {
                        clip_id: drag_id,
                        offset_px,
                        current_x,
                        ..
                    } if *drag_id == clip.id => (current_x - offset_px, clip_width),
                    TimelineInteraction::Resizing {
                        clip_id: resize_id,
                        current_x,
                        ..
                    } if *resize_id == clip.id => (clip_start_px, current_x - clip_start_px),
                    _ => (clip_start_px, clip_width),
                };

                let color = color_from_uuid(clip.asset_id);
                frame.fill_rectangle(
                    Point::new(draw_x, track_top + 2.0),
                    Size::new(draw_width.max(4.0), TRACK_HEIGHT - 4.0),
                    color,
                );

                // Right resize edge indicator
                frame.fill_rectangle(
                    Point::new(
                        draw_x + draw_width.max(4.0) - CLIP_RESIZE_EDGE_WIDTH,
                        track_top + 2.0,
                    ),
                    Size::new(CLIP_RESIZE_EDGE_WIDTH, TRACK_HEIGHT - 4.0),
                    Color {
                        a: 0.3,
                        ..Color::WHITE
                    },
                );

                // Clip duration label
                let dur = clip.duration().as_secs_f64();
                if clip_width > 30.0 {
                    frame.fill_text(canvas::Text {
                        content: format!("{:.1}s", dur),
                        position: Point::new(draw_x + 4.0, track_top + 18.0),
                        color: Color::WHITE,
                        size: iced::Pixels(11.0),
                        ..canvas::Text::default()
                    });
                }
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

        // Ghost preview hint for selected asset placement
        if self.selected_asset_id.is_some() {
            frame.fill_text(canvas::Text {
                content: "Click to place clip".into(),
                position: Point::new(bounds.width / 2.0 - 60.0, 4.0),
                color: Color::from_rgb(0.8, 0.8, 0.2),
                size: iced::Pixels(12.0),
                ..canvas::Text::default()
            });
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

        if self.selected_asset_id.is_some() {
            return mouse::Interaction::Crosshair;
        }

        if let Some(cursor_pos) = cursor.position_in(bounds) {
            if let Some((_, _, zone)) = self.hit_test_clip(cursor_pos.x, cursor_pos.y) {
                return match zone {
                    HitZone::Body => mouse::Interaction::Grab,
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
    use zeditor_core::timeline::{Clip, TimeRange, Timeline};

    fn make_test_timeline() -> Timeline {
        let mut tl = Timeline::new();
        tl.add_track("Video 1");

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
            selected_asset_id: None,
            zoom: 100.0,
            scroll_offset: 0.0,
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
            selected_asset_id: None,
            zoom: 100.0,
            scroll_offset: 50.0,
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
            selected_asset_id: None,
            zoom: 100.0,
            scroll_offset: 0.0,
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
            selected_asset_id: None,
            zoom: 100.0,
            scroll_offset: 0.0,
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
            selected_asset_id: None,
            zoom: 100.0,
            scroll_offset: 0.0,
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
}
