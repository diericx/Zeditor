use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::effects::EffectInstance;
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
    /// Link ID for paired clips (e.g. video+audio from same source).
    /// Clips with the same link_id move/resize/cut together.
    #[serde(default)]
    pub link_id: Option<Uuid>,
    /// Effects applied to this clip.
    #[serde(default)]
    pub effects: Vec<EffectInstance>,
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
            link_id: None,
            effects: Vec::new(),
        }
    }

    pub fn duration(&self) -> Duration {
        self.timeline_range.duration()
    }
}

/// Preview of what trimming would happen to a clip during a drag operation.
#[derive(Debug, Clone)]
pub struct TrimPreview {
    pub clip_id: Uuid,
    pub original_start: f64,
    pub original_end: f64,
    /// None means the clip (or this piece) would be fully removed.
    pub trimmed_start: Option<f64>,
    /// None means the clip (or this piece) would be fully removed.
    pub trimmed_end: Option<f64>,
}

/// Whether a track holds video or audio clips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackType {
    #[default]
    Video,
    Audio,
}

/// A track containing an ordered sequence of non-overlapping clips.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Track {
    pub name: String,
    pub clips: Vec<Clip>,
    #[serde(default)]
    pub track_type: TrackType,
    /// Tracks sharing a group_id are grouped (e.g. Video 1 + Audio 1).
    #[serde(default)]
    pub group_id: Option<Uuid>,
}

impl Track {
    pub fn new(name: impl Into<String>, track_type: TrackType) -> Self {
        Self {
            name: name.into(),
            clips: Vec::new(),
            track_type,
            group_id: None,
        }
    }

    pub fn video(name: impl Into<String>) -> Self {
        Self::new(name, TrackType::Video)
    }

    pub fn audio(name: impl Into<String>) -> Self {
        Self::new(name, TrackType::Audio)
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
                // Existing clip spans the entire new clip → split into left + right.
                // Preserve link_id on both pieces; the Timeline-level method handles
                // mirroring the split on linked tracks and reassigning link_ids.
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
                    link_id: existing.link_id,
                    effects: existing.effects.clone(),
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

    /// Preview what trimming would happen if a new clip [new_start, new_end) were placed.
    /// Returns a list of TrimPreview entries for each affected clip.
    /// `exclude_id` skips the clip being dragged (so it doesn't preview trimming itself).
    pub fn preview_trim_overlaps(
        &self,
        new_start: f64,
        new_end: f64,
        exclude_id: Option<Uuid>,
    ) -> Vec<TrimPreview> {
        let mut previews = Vec::new();

        for clip in &self.clips {
            if Some(clip.id) == exclude_id {
                continue;
            }

            let ex_start = clip.timeline_range.start.as_secs_f64();
            let ex_end = clip.timeline_range.end.as_secs_f64();

            // Check overlap
            if ex_start >= new_end || ex_end <= new_start {
                continue;
            }

            if ex_start < new_start && ex_end > new_end {
                // Clip spans entire new range → split into two pieces
                previews.push(TrimPreview {
                    clip_id: clip.id,
                    original_start: ex_start,
                    original_end: ex_end,
                    trimmed_start: Some(ex_start),
                    trimmed_end: Some(new_start),
                });
                previews.push(TrimPreview {
                    clip_id: clip.id,
                    original_start: ex_start,
                    original_end: ex_end,
                    trimmed_start: Some(new_end),
                    trimmed_end: Some(ex_end),
                });
            } else if ex_start >= new_start && ex_end <= new_end {
                // Fully covered → removed
                previews.push(TrimPreview {
                    clip_id: clip.id,
                    original_start: ex_start,
                    original_end: ex_end,
                    trimmed_start: None,
                    trimmed_end: None,
                });
            } else if ex_start < new_start {
                // Trim right side
                previews.push(TrimPreview {
                    clip_id: clip.id,
                    original_start: ex_start,
                    original_end: ex_end,
                    trimmed_start: Some(ex_start),
                    trimmed_end: Some(new_start),
                });
            } else {
                // Trim left side
                previews.push(TrimPreview {
                    clip_id: clip.id,
                    original_start: ex_start,
                    original_end: ex_end,
                    trimmed_start: Some(new_end),
                    trimmed_end: Some(ex_end),
                });
            }
        }

        previews
    }

    /// Preview what snap position a clip would get if placed at [clip_start, clip_end).
    /// Uses the trim previews for affected clips and original positions for unaffected ones.
    /// Returns the snapped start position if within threshold, else None.
    pub fn preview_snap_position(
        &self,
        clip_start: f64,
        clip_end: f64,
        exclude_id: Option<Uuid>,
        trim_previews: &[TrimPreview],
        snap_threshold_secs: f64,
    ) -> Option<f64> {
        let clip_duration = clip_end - clip_start;
        let mut best_gap = f64::MAX;
        let mut best_start = clip_start;
        let mut found = false;

        let affected: std::collections::HashSet<Uuid> =
            trim_previews.iter().map(|p| p.clip_id).collect();

        // Collect all neighbor edges to check against
        let mut edges: Vec<(f64, f64)> = Vec::new();

        for clip in &self.clips {
            if Some(clip.id) == exclude_id {
                continue;
            }
            if affected.contains(&clip.id) {
                continue; // use trimmed edges instead
            }
            edges.push((
                clip.timeline_range.start.as_secs_f64(),
                clip.timeline_range.end.as_secs_f64(),
            ));
        }

        for preview in trim_previews {
            if let (Some(ts), Some(te)) = (preview.trimmed_start, preview.trimmed_end) {
                edges.push((ts, te));
            }
        }

        for (other_start, other_end) in edges {
            // Snap our start to other's end
            let gap = (clip_start - other_end).abs();
            if gap <= snap_threshold_secs && gap < best_gap {
                best_gap = gap;
                best_start = other_end;
                found = true;
            }

            // Snap our end to other's start
            let gap = (clip_end - other_start).abs();
            if gap <= snap_threshold_secs && gap < best_gap {
                best_gap = gap;
                best_start = other_start - clip_duration;
                found = true;
            }
        }

        if found {
            Some(best_start)
        } else {
            None
        }
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Timeline {
    pub tracks: Vec<Track>,
}

impl Timeline {
    pub fn new() -> Self {
        Self { tracks: Vec::new() }
    }

    pub fn add_track(&mut self, name: impl Into<String>, track_type: TrackType) -> usize {
        let idx = self.tracks.len();
        self.tracks.push(Track::new(name, track_type));
        idx
    }

    pub fn add_track_with_group(
        &mut self,
        name: impl Into<String>,
        track_type: TrackType,
        group_id: Option<Uuid>,
    ) -> usize {
        let idx = self.tracks.len();
        let mut track = Track::new(name, track_type);
        track.group_id = group_id;
        self.tracks.push(track);
        idx
    }

    /// Find all track indices that share the same group_id as the given track.
    pub fn group_members(&self, track_index: usize) -> Vec<usize> {
        let group_id = match self.tracks.get(track_index) {
            Some(track) => track.group_id,
            None => return vec![],
        };
        match group_id {
            Some(gid) => self
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.group_id == Some(gid))
                .map(|(i, _)| i)
                .collect(),
            None => vec![track_index],
        }
    }

    /// Find all clips across all tracks that share the given link_id.
    /// Returns (track_index, clip_id) pairs.
    pub fn find_linked_clips(&self, link_id: Uuid) -> Vec<(usize, Uuid)> {
        let mut result = Vec::new();
        for (i, track) in self.tracks.iter().enumerate() {
            for clip in &track.clips {
                if clip.link_id == Some(link_id) {
                    result.push((i, clip.id));
                }
            }
        }
        result
    }

    /// Find the audio track paired with a video track via group_id.
    pub fn find_paired_audio_track(&self, video_track_index: usize) -> Option<usize> {
        let group_id = self.tracks.get(video_track_index)?.group_id?;
        self.tracks
            .iter()
            .enumerate()
            .find(|(i, t)| *i != video_track_index && t.group_id == Some(group_id) && t.track_type == TrackType::Audio)
            .map(|(i, _)| i)
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
    /// If any existing linked clips are split, the linked partner on its paired
    /// track is also split at the same boundaries, and new link_ids are assigned
    /// so left pieces stay in one group and right pieces form a new group.
    pub fn add_clip_trimming_overlaps(&mut self, track_index: usize, clip: Clip) -> Result<()> {
        let new_start = clip.timeline_range.start;
        let new_end = clip.timeline_range.end;

        // Pre-scan: identify linked clips on this track that will be split
        // (those whose range fully spans the new clip's range).
        let mut split_link_ids: Vec<Uuid> = Vec::new();
        {
            let track = self.track(track_index)?;
            for existing in &track.clips {
                if existing.timeline_range.start < new_start
                    && existing.timeline_range.end > new_end
                {
                    if let Some(link_id) = existing.link_id {
                        split_link_ids.push(link_id);
                    }
                }
            }
        }

        // Perform the add/split on the primary track
        self.track_mut(track_index)?.add_clip_trimming_overlaps(clip);

        // Mirror each split on linked partner tracks
        for old_link_id in &split_link_ids {
            let linked = self.find_linked_clips(*old_link_id);
            for (linked_track_idx, linked_clip_id) in linked {
                if linked_track_idx == track_index {
                    continue; // skip the already-split pieces on the primary track
                }

                // Check if the linked clip needs cutting at new_start
                let needs_start_cut = self.track(linked_track_idx)
                    .ok()
                    .and_then(|t| t.get_clip(linked_clip_id))
                    .map(|c| {
                        c.timeline_range.start < new_start
                            && c.timeline_range.end > new_start
                    })
                    .unwrap_or(false);

                if needs_start_cut {
                    let _ = self.cut_at(linked_track_idx, new_start);
                }

                // Check if the clip at new_end needs cutting (may be a different
                // clip now after the first cut)
                let needs_end_cut = self.track(linked_track_idx)
                    .ok()
                    .and_then(|t| t.clip_at(new_end))
                    .map(|c| {
                        c.link_id == Some(*old_link_id)
                            && c.timeline_range.start < new_end
                            && c.timeline_range.end > new_end
                    })
                    .unwrap_or(false);

                if needs_end_cut {
                    let _ = self.cut_at(linked_track_idx, new_end);
                }
            }

            // Reassign link_ids: left pieces keep old, right pieces get new,
            // middle pieces (between new_start..new_end) lose their link.
            let new_right_link = Uuid::new_v4();
            let all_with_link = self.find_linked_clips(*old_link_id);

            for (ti, ci) in all_with_link {
                let clip_start = match self.track(ti)
                    .ok()
                    .and_then(|t| t.get_clip(ci))
                {
                    Some(c) => c.timeline_range.start,
                    None => continue,
                };

                if clip_start >= new_end {
                    // Right piece → new group
                    if let Some(c) = self.track_mut(ti).ok().and_then(|t| t.get_clip_mut(ci)) {
                        c.link_id = Some(new_right_link);
                    }
                } else if clip_start >= new_start {
                    // Middle piece → orphaned (no partner on the other track)
                    if let Some(c) = self.track_mut(ti).ok().and_then(|t| t.get_clip_mut(ci)) {
                        c.link_id = None;
                    }
                }
                // Left piece (clip_start < new_start) keeps old link_id
            }
        }

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

        let clip_link_id = clip.link_id;
        let clip_effects = clip.effects.clone();

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
            link_id: clip_link_id,
            effects: clip_effects.clone(),
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
            link_id: clip_link_id,
            effects: clip_effects,
        };

        let left_id = left.id;
        let right_id = right.id;

        track.clips.remove(clip_idx);
        track.clips.insert(clip_idx, right);
        track.clips.insert(clip_idx, left);

        Ok((left_id, right_id))
    }

    /// Move a clip from one track/position to another, trimming overlapping clips.
    /// If the move splits a linked clip, the linked partner is also split.
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

        self.add_clip_trimming_overlaps(dest_track, clip)?;
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

    /// Add a clip to both video and audio tracks with a shared link_id.
    /// Returns (video_clip_id, audio_clip_id).
    pub fn add_clip_with_audio(
        &mut self,
        video_track: usize,
        audio_track: usize,
        asset_id: Uuid,
        position: TimelinePosition,
        source_range: TimeRange,
    ) -> Result<(Uuid, Uuid)> {
        let link_id = Uuid::new_v4();

        let mut video_clip = Clip::new(asset_id, position, source_range);
        video_clip.link_id = Some(link_id);
        let video_clip_id = video_clip.id;

        let mut audio_clip = Clip::new(asset_id, position, source_range);
        audio_clip.link_id = Some(link_id);
        let audio_clip_id = audio_clip.id;

        self.add_clip_trimming_overlaps(video_track, video_clip)?;
        self.add_clip_trimming_overlaps(audio_track, audio_clip)?;

        Ok((video_clip_id, audio_clip_id))
    }

    /// Move a clip and all its linked clips by the same delta.
    pub fn move_clip_grouped(
        &mut self,
        source_track: usize,
        clip_id: Uuid,
        dest_track: usize,
        new_position: TimelinePosition,
    ) -> Result<()> {
        // Get the original clip's position and link_id
        let (old_position, link_id) = {
            let track = self.track(source_track)?;
            let clip = track.get_clip(clip_id).ok_or(CoreError::ClipNotFound(clip_id))?;
            (clip.timeline_range.start, clip.link_id)
        };

        // Move the primary clip
        self.move_clip(source_track, clip_id, dest_track, new_position)?;

        // If linked, move linked clips by same delta
        if let Some(link_id) = link_id {
            let delta_secs = new_position.as_secs_f64() - old_position.as_secs_f64();
            let linked = self.find_linked_clips(link_id);
            for (track_idx, linked_clip_id) in linked {
                if linked_clip_id == clip_id {
                    continue;
                }
                let linked_pos = {
                    let track = self.track(track_idx)?;
                    match track.get_clip(linked_clip_id) {
                        Some(clip) => clip.timeline_range.start,
                        None => continue, // linked clip was deleted independently
                    }
                };
                let new_linked_pos = TimelinePosition::from_secs_f64(
                    (linked_pos.as_secs_f64() + delta_secs).max(0.0)
                );
                self.move_clip(track_idx, linked_clip_id, track_idx, new_linked_pos)?;
            }
        }

        Ok(())
    }

    /// Resize a clip and all its linked clips by the same delta.
    pub fn resize_clip_grouped(
        &mut self,
        track_index: usize,
        clip_id: Uuid,
        new_end: TimelinePosition,
    ) -> Result<()> {
        // Get old end and link_id
        let (old_end, link_id) = {
            let track = self.track(track_index)?;
            let clip = track.get_clip(clip_id).ok_or(CoreError::ClipNotFound(clip_id))?;
            (clip.timeline_range.end, clip.link_id)
        };

        // Resize primary clip
        self.resize_clip(track_index, clip_id, new_end)?;

        // If linked, resize linked clips by same delta
        if let Some(link_id) = link_id {
            let delta_secs = new_end.as_secs_f64() - old_end.as_secs_f64();
            let linked = self.find_linked_clips(link_id);
            for (track_idx, linked_clip_id) in linked {
                if linked_clip_id == clip_id {
                    continue;
                }
                let linked_end = {
                    let track = self.track(track_idx)?;
                    match track.get_clip(linked_clip_id) {
                        Some(clip) => clip.timeline_range.end,
                        None => continue, // linked clip was deleted independently
                    }
                };
                let new_linked_end = TimelinePosition::from_secs_f64(
                    linked_end.as_secs_f64() + delta_secs
                );
                self.resize_clip(track_idx, linked_clip_id, new_linked_end)?;
            }
        }

        Ok(())
    }

    /// Cut at a position, splitting linked clips on all their tracks.
    /// Returns the (left_id, right_id) pairs for all affected clips.
    pub fn cut_at_grouped(
        &mut self,
        track_index: usize,
        position: TimelinePosition,
    ) -> Result<Vec<(Uuid, Uuid)>> {
        // Find the clip at position and get its link_id
        let link_id = {
            let track = self.track(track_index)?;
            let clip = track
                .clip_at(position)
                .ok_or(CoreError::CutOutsideClip { position })?;
            clip.link_id
        };

        // Cut the primary clip
        let (left_id, right_id) = self.cut_at(track_index, position)?;

        let mut results = vec![(left_id, right_id)];

        if let Some(link_id) = link_id {
            // Find other linked clips (before cut)
            let linked = self.find_linked_clips(link_id);
            for (track_idx, linked_clip_id) in linked {
                if track_idx == track_index {
                    // These are the newly-cut clips (left/right), skip
                    continue;
                }
                // Check if this linked clip contains the cut position
                let contains = {
                    let track = self.track(track_idx)?;
                    if let Some(clip) = track.get_clip(linked_clip_id) {
                        clip.timeline_range.contains(position)
                    } else {
                        false
                    }
                };
                if contains {
                    let (l, r) = self.cut_at(track_idx, position)?;
                    results.push((l, r));
                }
            }

            // Assign new matching link_ids: left clips get one link_id, right clips get another
            let new_left_link = Uuid::new_v4();
            let new_right_link = Uuid::new_v4();
            for (l_id, r_id) in &results {
                // Find and update left clip's link_id
                for track in &mut self.tracks {
                    if let Some(clip) = track.get_clip_mut(*l_id) {
                        clip.link_id = Some(new_left_link);
                    }
                    if let Some(clip) = track.get_clip_mut(*r_id) {
                        clip.link_id = Some(new_right_link);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Find all clips across all tracks that reference the given asset_id.
    /// Returns (track_index, clip_id) pairs.
    pub fn clips_using_asset(&self, asset_id: Uuid) -> Vec<(usize, Uuid)> {
        let mut result = Vec::new();
        for (i, track) in self.tracks.iter().enumerate() {
            for clip in &track.clips {
                if clip.asset_id == asset_id {
                    result.push((i, clip.id));
                }
            }
        }
        result
    }

    /// Remove all clips that reference the given asset_id.
    /// Returns the count of clips removed.
    pub fn remove_clips_by_asset(&mut self, asset_id: Uuid) -> usize {
        let mut count = 0;
        for track in &mut self.tracks {
            let before = track.clips.len();
            track.clips.retain(|c| c.asset_id != asset_id);
            count += before - track.clips.len();
        }
        count
    }

    /// Remove a clip and any linked partner clips.
    pub fn remove_clip_grouped(&mut self, track_index: usize, clip_id: Uuid) -> Result<()> {
        let link_id = self
            .track(track_index)?
            .get_clip(clip_id)
            .ok_or(CoreError::ClipNotFound(clip_id))?
            .link_id;

        // Remove the primary clip
        self.track_mut(track_index)?.remove_clip(clip_id)?;

        // Remove linked clips if any
        if let Some(link_id) = link_id {
            let linked = self.find_linked_clips(link_id);
            for (linked_track, linked_clip_id) in linked {
                let _ = self.track_mut(linked_track).ok().map(|t| t.remove_clip(linked_clip_id));
            }
        }

        Ok(())
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
