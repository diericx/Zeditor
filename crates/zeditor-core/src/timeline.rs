use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{CoreError, Result};

/// A position on the timeline, represented as a duration from the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TimelinePosition(Duration);

impl TimelinePosition {
    pub fn zero() -> Self {
        Self(Duration::ZERO)
    }

    pub fn from_secs_f64(secs: f64) -> Self {
        Self(Duration::from_secs_f64(secs))
    }

    pub fn from_millis(millis: u64) -> Self {
        Self(Duration::from_millis(millis))
    }

    pub fn as_duration(&self) -> Duration {
        self.0
    }

    pub fn as_secs_f64(&self) -> f64 {
        self.0.as_secs_f64()
    }
}

impl std::ops::Add for TimelinePosition {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for TimelinePosition {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

/// A time range with start (inclusive) and end (exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: TimelinePosition,
    pub end: TimelinePosition,
}

impl TimeRange {
    pub fn new(start: TimelinePosition, end: TimelinePosition) -> Result<Self> {
        if start >= end {
            return Err(CoreError::InvalidTimeRange { start, end });
        }
        Ok(Self { start, end })
    }

    pub fn duration(&self) -> Duration {
        self.end.as_duration() - self.start.as_duration()
    }

    pub fn contains(&self, pos: TimelinePosition) -> bool {
        pos >= self.start && pos < self.end
    }

    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// A clip placed on a track, referencing a portion of a media asset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Clip {
    pub id: Uuid,
    /// The media asset this clip references.
    pub asset_id: Uuid,
    /// Where this clip sits on the timeline.
    pub timeline_range: TimeRange,
    /// The portion of the source media used (in/out points).
    pub source_range: TimeRange,
}

impl Clip {
    pub fn new(
        asset_id: Uuid,
        timeline_start: TimelinePosition,
        source_range: TimeRange,
    ) -> Self {
        let duration_pos = TimelinePosition(source_range.duration());
        let timeline_end = timeline_start + duration_pos;
        let timeline_range = TimeRange {
            start: timeline_start,
            end: timeline_end,
        };
        Self {
            id: Uuid::new_v4(),
            asset_id,
            timeline_range,
            source_range,
        }
    }

    pub fn duration(&self) -> Duration {
        self.timeline_range.duration()
    }
}

/// A track containing an ordered sequence of non-overlapping clips.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Track {
    pub name: String,
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            clips: Vec::new(),
        }
    }

    /// Add a clip, checking for overlaps with existing clips.
    pub fn add_clip(&mut self, clip: Clip) -> Result<()> {
        for existing in &self.clips {
            if existing.timeline_range.overlaps(&clip.timeline_range) {
                return Err(CoreError::ClipOverlap {
                    position: clip.timeline_range.start,
                });
            }
        }
        self.clips.push(clip);
        self.clips
            .sort_by_key(|c| c.timeline_range.start.as_duration());
        Ok(())
    }

    /// Remove a clip by id, returning it.
    pub fn remove_clip(&mut self, clip_id: Uuid) -> Result<Clip> {
        let idx = self
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .ok_or(CoreError::ClipNotFound(clip_id))?;
        Ok(self.clips.remove(idx))
    }

    pub fn get_clip(&self, clip_id: Uuid) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == clip_id)
    }

    pub fn get_clip_mut(&mut self, clip_id: Uuid) -> Option<&mut Clip> {
        self.clips.iter_mut().find(|c| c.id == clip_id)
    }

    /// Find the clip at the given position.
    pub fn clip_at(&self, pos: TimelinePosition) -> Option<&Clip> {
        self.clips
            .iter()
            .find(|c| c.timeline_range.contains(pos))
    }

    /// Add a clip, trimming any existing overlapping clips to make room.
    ///
    /// - If an existing clip starts before the new clip → trim its end to the new clip's start
    /// - If an existing clip is fully inside the new clip → remove it
    /// - If an existing clip starts within the new clip but extends past → trim its start to the new clip's end
    /// - If an existing clip spans the entire new clip → split it into left and right pieces
    pub fn add_clip_trimming_overlaps(&mut self, new_clip: Clip) {
        let new_start = new_clip.timeline_range.start;
        let new_end = new_clip.timeline_range.end;

        let mut to_remove = Vec::new();
        let mut to_add = Vec::new();

        for (i, existing) in self.clips.iter_mut().enumerate() {
            if !existing.timeline_range.overlaps(&new_clip.timeline_range) {
                continue;
            }

            let ex_start = existing.timeline_range.start;
            let ex_end = existing.timeline_range.end;

            if ex_start < new_start && ex_end > new_end {
                // Existing clip spans the entire new clip → split into left + right
                let right_source_start = TimelinePosition(
                    existing.source_range.start.as_duration()
                        + (new_end.as_duration() - ex_start.as_duration()),
                );
                let right_piece = Clip {
                    id: Uuid::new_v4(),
                    asset_id: existing.asset_id,
                    timeline_range: TimeRange {
                        start: new_end,
                        end: ex_end,
                    },
                    source_range: TimeRange {
                        start: right_source_start,
                        end: existing.source_range.end,
                    },
                };
                to_add.push(right_piece);

                // Trim existing in-place to be the left piece
                let left_duration =
                    new_start.as_duration() - ex_start.as_duration();
                existing.timeline_range.end = new_start;
                existing.source_range.end = TimelinePosition(
                    existing.source_range.start.as_duration() + left_duration,
                );
            } else if ex_start >= new_start && ex_end <= new_end {
                // Fully covered — mark for removal
                to_remove.push(i);
            } else if ex_start < new_start {
                // Existing starts before new clip, ends inside → trim end
                let trimmed_duration =
                    new_start.as_duration() - ex_start.as_duration();
                existing.timeline_range.end = new_start;
                existing.source_range.end = TimelinePosition(
                    existing.source_range.start.as_duration() + trimmed_duration,
                );
            } else {
                // Existing starts inside new clip, extends past → trim start
                let cut_amount =
                    new_end.as_duration() - ex_start.as_duration();
                existing.timeline_range.start = new_end;
                existing.source_range.start = TimelinePosition(
                    existing.source_range.start.as_duration() + cut_amount,
                );
            }
        }

        // Remove fully covered clips (reverse order to preserve indices)
        for i in to_remove.into_iter().rev() {
            self.clips.remove(i);
        }

        // Add right pieces from splits and the new clip
        self.clips.extend(to_add);
        self.clips.push(new_clip);
        self.clips
            .sort_by_key(|c| c.timeline_range.start.as_duration());
    }

    /// Get the end position of the last clip on this track.
    pub fn end_position(&self) -> TimelinePosition {
        self.clips
            .last()
            .map(|c| c.timeline_range.end)
            .unwrap_or(TimelinePosition::zero())
    }
}

/// The timeline containing all tracks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Timeline {
    pub tracks: Vec<Track>,
}

impl Timeline {
    pub fn new() -> Self {
        Self { tracks: Vec::new() }
    }

    pub fn add_track(&mut self, name: impl Into<String>) -> usize {
        let idx = self.tracks.len();
        self.tracks.push(Track::new(name));
        idx
    }

    pub fn track(&self, index: usize) -> Result<&Track> {
        self.tracks.get(index).ok_or(CoreError::TrackNotFound(index))
    }

    pub fn track_mut(&mut self, index: usize) -> Result<&mut Track> {
        self.tracks
            .get_mut(index)
            .ok_or(CoreError::TrackNotFound(index))
    }

    /// Add a clip to the specified track.
    pub fn add_clip(&mut self, track_index: usize, clip: Clip) -> Result<()> {
        self.track_mut(track_index)?.add_clip(clip)
    }

    /// Add a clip to the specified track, trimming overlapping clips to make room.
    pub fn add_clip_trimming_overlaps(&mut self, track_index: usize, clip: Clip) -> Result<()> {
        self.track_mut(track_index)?.add_clip_trimming_overlaps(clip);
        Ok(())
    }

    /// Cut the clip at the given position, splitting it into two clips.
    /// Returns the ids of the two resulting clips (left, right).
    pub fn cut_at(
        &mut self,
        track_index: usize,
        position: TimelinePosition,
    ) -> Result<(Uuid, Uuid)> {
        let track = self.track_mut(track_index)?;

        let clip_idx = track
            .clips
            .iter()
            .position(|c| c.timeline_range.contains(position))
            .ok_or(CoreError::CutOutsideClip { position })?;

        let clip = &track.clips[clip_idx];

        // Ensure cut is not at the very start or end of the clip.
        if position == clip.timeline_range.start || position >= clip.timeline_range.end {
            return Err(CoreError::CutOutsideClip { position });
        }

        let offset_in_clip = position.as_duration() - clip.timeline_range.start.as_duration();

        let source_split =
            TimelinePosition(clip.source_range.start.as_duration() + offset_in_clip);

        // Left clip: original start to cut position.
        let left = Clip {
            id: Uuid::new_v4(),
            asset_id: clip.asset_id,
            timeline_range: TimeRange {
                start: clip.timeline_range.start,
                end: position,
            },
            source_range: TimeRange {
                start: clip.source_range.start,
                end: source_split,
            },
        };

        // Right clip: cut position to original end.
        let right = Clip {
            id: Uuid::new_v4(),
            asset_id: clip.asset_id,
            timeline_range: TimeRange {
                start: position,
                end: clip.timeline_range.end,
            },
            source_range: TimeRange {
                start: source_split,
                end: clip.source_range.end,
            },
        };

        let left_id = left.id;
        let right_id = right.id;

        track.clips.remove(clip_idx);
        track.clips.insert(clip_idx, right);
        track.clips.insert(clip_idx, left);

        Ok((left_id, right_id))
    }

    /// Move a clip from one track/position to another, trimming overlapping clips.
    pub fn move_clip(
        &mut self,
        source_track: usize,
        clip_id: Uuid,
        dest_track: usize,
        new_position: TimelinePosition,
    ) -> Result<()> {
        let mut clip = self.track_mut(source_track)?.remove_clip(clip_id)?;

        let duration_pos = TimelinePosition(clip.timeline_range.duration());
        clip.timeline_range = TimeRange {
            start: new_position,
            end: new_position + duration_pos,
        };

        self.track_mut(dest_track)?.add_clip_trimming_overlaps(clip);
        Ok(())
    }

    /// Resize a clip by changing its timeline end (and adjusting source out point).
    pub fn resize_clip(
        &mut self,
        track_index: usize,
        clip_id: Uuid,
        new_end: TimelinePosition,
    ) -> Result<()> {
        let track = self.track_mut(track_index)?;
        let clip = track
            .get_clip_mut(clip_id)
            .ok_or(CoreError::ClipNotFound(clip_id))?;

        if new_end <= clip.timeline_range.start {
            return Err(CoreError::InvalidTimeRange {
                start: clip.timeline_range.start,
                end: new_end,
            });
        }

        let new_duration = new_end.as_duration() - clip.timeline_range.start.as_duration();
        let new_source_end = TimelinePosition(clip.source_range.start.as_duration() + new_duration);

        clip.timeline_range.end = new_end;
        clip.source_range.end = new_source_end;

        // Check for overlaps with other clips after resize.
        let clip_range = clip.timeline_range;
        let clip_id = clip.id;
        for other in &track.clips {
            if other.id != clip_id && other.timeline_range.overlaps(&clip_range) {
                return Err(CoreError::ClipOverlap {
                    position: clip_range.start,
                });
            }
        }

        Ok(())
    }

    /// Snap a clip to be adjacent to the nearest clip on the same track.
    /// Returns the new position if snapped, None if no adjacent clips.
    pub fn snap_to_adjacent(
        &mut self,
        track_index: usize,
        clip_id: Uuid,
        snap_threshold: Duration,
    ) -> Result<Option<TimelinePosition>> {
        let track = self.track(track_index)?;
        let clip = track
            .get_clip(clip_id)
            .ok_or(CoreError::ClipNotFound(clip_id))?;

        let clip_start = clip.timeline_range.start;
        let clip_end = clip.timeline_range.end;
        let mut best_snap: Option<(Duration, TimelinePosition)> = None;

        for other in &track.clips {
            if other.id == clip_id {
                continue;
            }

            // Check snapping our start to other's end.
            let gap = if clip_start.as_duration() >= other.timeline_range.end.as_duration() {
                clip_start.as_duration() - other.timeline_range.end.as_duration()
            } else {
                other.timeline_range.end.as_duration() - clip_start.as_duration()
            };
            if gap <= snap_threshold {
                if best_snap.is_none() || gap < best_snap.unwrap().0 {
                    best_snap = Some((gap, other.timeline_range.end));
                }
            }

            // Check snapping our end to other's start.
            let gap = if clip_end.as_duration() >= other.timeline_range.start.as_duration() {
                clip_end.as_duration() - other.timeline_range.start.as_duration()
            } else {
                other.timeline_range.start.as_duration() - clip_end.as_duration()
            };
            if gap <= snap_threshold {
                let snap_pos = TimelinePosition(
                    other.timeline_range.start.as_duration() - clip.timeline_range.duration(),
                );
                if best_snap.is_none() || gap < best_snap.unwrap().0 {
                    best_snap = Some((gap, snap_pos));
                }
            }
        }

        if let Some((_gap, new_start)) = best_snap {
            let track = self.track_mut(track_index)?;
            let clip = track
                .get_clip_mut(clip_id)
                .ok_or(CoreError::ClipNotFound(clip_id))?;
            let duration = clip.timeline_range.duration();
            clip.timeline_range.start = new_start;
            clip.timeline_range.end = TimelinePosition(new_start.as_duration() + duration);
            Ok(Some(new_start))
        } else {
            Ok(None)
        }
    }

    /// Get the total duration of the timeline (end of last clip across all tracks).
    pub fn duration(&self) -> Duration {
        self.tracks
            .iter()
            .map(|t| t.end_position().as_duration())
            .max()
            .unwrap_or(Duration::ZERO)
    }
}
