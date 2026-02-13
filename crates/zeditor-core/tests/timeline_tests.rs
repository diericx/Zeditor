use std::time::Duration;

use uuid::Uuid;
use zeditor_core::timeline::*;

fn make_clip(asset_id: Uuid, start_secs: f64, duration_secs: f64) -> Clip {
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(duration_secs),
    )
    .unwrap();
    Clip::new(asset_id, TimelinePosition::from_secs_f64(start_secs), source_range)
}

#[test]
fn test_add_clip_to_track() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    let clip = make_clip(asset_id, 0.0, 5.0);

    timeline.add_clip(0, clip).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 1);
}

#[test]
fn test_add_multiple_clips_no_overlap() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();
    timeline
        .add_clip(0, make_clip(asset_id, 5.0, 3.0))
        .unwrap();
    timeline
        .add_clip(0, make_clip(asset_id, 10.0, 2.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 3);
}

#[test]
fn test_add_clip_overlap_fails() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();

    let result = timeline.add_clip(0, make_clip(asset_id, 3.0, 5.0));
    assert!(result.is_err());
}

#[test]
fn test_cut_clip() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    let clip = make_clip(asset_id, 0.0, 10.0);
    timeline.add_clip(0, clip).unwrap();

    let (left_id, right_id) =
        timeline.cut_at(0, TimelinePosition::from_secs_f64(4.0)).unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 2);

    let left = timeline.tracks[0].get_clip(left_id).unwrap();
    let right = timeline.tracks[0].get_clip(right_id).unwrap();

    // Left clip: 0..4
    assert_eq!(left.timeline_range.start, TimelinePosition::from_secs_f64(0.0));
    assert_eq!(left.timeline_range.end, TimelinePosition::from_secs_f64(4.0));

    // Right clip: 4..10
    assert_eq!(right.timeline_range.start, TimelinePosition::from_secs_f64(4.0));
    assert_eq!(right.timeline_range.end, TimelinePosition::from_secs_f64(10.0));
}

#[test]
fn test_cut_at_clip_boundary_fails() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();

    // Cut at the start of the clip should fail.
    let result = timeline.cut_at(0, TimelinePosition::from_secs_f64(0.0));
    assert!(result.is_err());

    // Cut at the end of the clip should fail.
    let result = timeline.cut_at(0, TimelinePosition::from_secs_f64(5.0));
    assert!(result.is_err());
}

#[test]
fn test_move_clip_same_track() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    let clip = make_clip(asset_id, 0.0, 5.0);
    let clip_id = clip.id;
    timeline.add_clip(0, clip).unwrap();

    timeline
        .move_clip(0, clip_id, 0, TimelinePosition::from_secs_f64(10.0))
        .unwrap();

    let moved = timeline.tracks[0].get_clip(clip_id).unwrap();
    assert_eq!(
        moved.timeline_range.start,
        TimelinePosition::from_secs_f64(10.0)
    );
    assert_eq!(
        moved.timeline_range.end,
        TimelinePosition::from_secs_f64(15.0)
    );
}

#[test]
fn test_move_clip_between_tracks() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);
    timeline.add_track("Video 2", TrackType::Video);

    let asset_id = Uuid::new_v4();
    let clip = make_clip(asset_id, 0.0, 5.0);
    let clip_id = clip.id;
    timeline.add_clip(0, clip).unwrap();

    timeline
        .move_clip(0, clip_id, 1, TimelinePosition::from_secs_f64(2.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 0);
    assert_eq!(timeline.tracks[1].clips.len(), 1);
    assert_eq!(timeline.tracks[1].clips[0].id, clip_id);
}

#[test]
fn test_resize_clip() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    let clip = make_clip(asset_id, 0.0, 5.0);
    let clip_id = clip.id;
    timeline.add_clip(0, clip).unwrap();

    timeline
        .resize_clip(0, clip_id, TimelinePosition::from_secs_f64(8.0))
        .unwrap();

    let resized = timeline.tracks[0].get_clip(clip_id).unwrap();
    assert_eq!(
        resized.timeline_range.end,
        TimelinePosition::from_secs_f64(8.0)
    );
}

#[test]
fn test_snap_to_adjacent() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Clip A at 0..5s
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();

    // Clip B at 5.05..10.05s (50ms gap from A)
    let clip_b = make_clip(asset_id, 5.05, 5.0);
    let clip_b_id = clip_b.id;
    timeline.add_clip(0, clip_b).unwrap();

    // Snap with 100ms threshold should close the gap.
    let result = timeline
        .snap_to_adjacent(0, clip_b_id, Duration::from_millis(100))
        .unwrap();

    assert!(result.is_some());
    let snapped = timeline.tracks[0].get_clip(clip_b_id).unwrap();
    assert_eq!(
        snapped.timeline_range.start,
        TimelinePosition::from_secs_f64(5.0)
    );
}

#[test]
fn test_snap_no_adjacent_within_threshold() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();

    // Clip B at 10..15s (5s gap from A, way beyond threshold)
    let clip_b = make_clip(asset_id, 10.0, 5.0);
    let clip_b_id = clip_b.id;
    timeline.add_clip(0, clip_b).unwrap();

    let result = timeline
        .snap_to_adjacent(0, clip_b_id, Duration::from_millis(100))
        .unwrap();

    assert!(result.is_none());
}

#[test]
fn test_timeline_duration() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);
    timeline.add_track("Video 2", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Track 0: clips up to 8s
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();
    timeline
        .add_clip(0, make_clip(asset_id, 5.0, 3.0))
        .unwrap();

    // Track 1: clips up to 12s
    timeline
        .add_clip(1, make_clip(asset_id, 0.0, 12.0))
        .unwrap();

    let dur = timeline.duration().as_secs_f64();
    assert!((dur - 12.0).abs() < 0.001);
}

#[test]
fn test_time_range_contains() {
    let range = TimeRange::new(
        TimelinePosition::from_secs_f64(2.0),
        TimelinePosition::from_secs_f64(5.0),
    )
    .unwrap();

    assert!(range.contains(TimelinePosition::from_secs_f64(2.0)));
    assert!(range.contains(TimelinePosition::from_secs_f64(3.0)));
    assert!(!range.contains(TimelinePosition::from_secs_f64(5.0))); // exclusive end
    assert!(!range.contains(TimelinePosition::from_secs_f64(1.0)));
}

#[test]
fn test_add_clip_trims_left_overlap() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [0, 10)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 10.0))
        .unwrap();

    // New clip [5, 15) — should trim existing to [0, 5)
    timeline
        .add_clip_trimming_overlaps(0, make_clip(asset_id, 5.0, 10.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 2);
    let first = &timeline.tracks[0].clips[0];
    assert_eq!(first.timeline_range.start, TimelinePosition::from_secs_f64(0.0));
    assert_eq!(first.timeline_range.end, TimelinePosition::from_secs_f64(5.0));
    // Source range should also be trimmed to 5s duration
    let src_dur = first.source_range.end.as_secs_f64() - first.source_range.start.as_secs_f64();
    assert!((src_dur - 5.0).abs() < 0.001);
}

#[test]
fn test_add_clip_removes_fully_covered() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [3, 7)
    timeline
        .add_clip(0, make_clip(asset_id, 3.0, 4.0))
        .unwrap();

    // New clip [0, 10) — fully covers existing, should remove it
    timeline
        .add_clip_trimming_overlaps(0, make_clip(asset_id, 0.0, 10.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 1);
    let clip = &timeline.tracks[0].clips[0];
    assert_eq!(clip.timeline_range.start, TimelinePosition::from_secs_f64(0.0));
    assert_eq!(clip.timeline_range.end, TimelinePosition::from_secs_f64(10.0));
}

#[test]
fn test_add_clip_trims_right_overlap() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [5, 15)
    timeline
        .add_clip(0, make_clip(asset_id, 5.0, 10.0))
        .unwrap();

    // New clip [0, 8) — should trim existing to [8, 15)
    timeline
        .add_clip_trimming_overlaps(0, make_clip(asset_id, 0.0, 8.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 2);
    // Clips should be sorted: [0,8) then [8,15)
    let first = &timeline.tracks[0].clips[0];
    assert_eq!(first.timeline_range.start, TimelinePosition::from_secs_f64(0.0));
    assert_eq!(first.timeline_range.end, TimelinePosition::from_secs_f64(8.0));

    let second = &timeline.tracks[0].clips[1];
    assert_eq!(second.timeline_range.start, TimelinePosition::from_secs_f64(8.0));
    assert_eq!(second.timeline_range.end, TimelinePosition::from_secs_f64(15.0));
    // Source range start should be trimmed by 3s (8-5)
    let src_start = second.source_range.start.as_secs_f64();
    assert!((src_start - 3.0).abs() < 0.001);
}

#[test]
fn test_add_clip_splits_spanning_clip() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [0, 20) with source [0, 20)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 20.0))
        .unwrap();

    // New clip [5, 10) — should split existing into [0,5) and [10,20)
    timeline
        .add_clip_trimming_overlaps(0, make_clip(asset_id, 5.0, 5.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 3);

    let left = &timeline.tracks[0].clips[0];
    assert_eq!(left.timeline_range.start, TimelinePosition::from_secs_f64(0.0));
    assert_eq!(left.timeline_range.end, TimelinePosition::from_secs_f64(5.0));
    let left_src_dur = left.source_range.end.as_secs_f64() - left.source_range.start.as_secs_f64();
    assert!((left_src_dur - 5.0).abs() < 0.001);

    let middle = &timeline.tracks[0].clips[1];
    assert_eq!(middle.timeline_range.start, TimelinePosition::from_secs_f64(5.0));
    assert_eq!(middle.timeline_range.end, TimelinePosition::from_secs_f64(10.0));

    let right = &timeline.tracks[0].clips[2];
    assert_eq!(right.timeline_range.start, TimelinePosition::from_secs_f64(10.0));
    assert_eq!(right.timeline_range.end, TimelinePosition::from_secs_f64(20.0));
    // Right piece source should start at 10s into the original
    let right_src_start = right.source_range.start.as_secs_f64();
    assert!((right_src_start - 10.0).abs() < 0.001);
    let right_src_end = right.source_range.end.as_secs_f64();
    assert!((right_src_end - 20.0).abs() < 0.001);
}

#[test]
fn test_move_clip_trims_overlap() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Clip A [0, 5), Clip B [5, 15)
    let clip_a = make_clip(asset_id, 0.0, 5.0);
    let clip_a_id = clip_a.id;
    timeline.add_clip(0, clip_a).unwrap();
    timeline
        .add_clip(0, make_clip(asset_id, 5.0, 10.0))
        .unwrap();

    // Move A to [3, 8) — overlaps B → should trim B's left side
    timeline
        .move_clip(0, clip_a_id, 0, TimelinePosition::from_secs_f64(3.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 2);
    let moved = timeline.tracks[0].get_clip(clip_a_id).unwrap();
    assert_eq!(moved.timeline_range.start, TimelinePosition::from_secs_f64(3.0));
    assert_eq!(moved.timeline_range.end, TimelinePosition::from_secs_f64(8.0));

    // B should be trimmed to [8, 15)
    let b = &timeline.tracks[0].clips[1];
    assert_eq!(b.timeline_range.start, TimelinePosition::from_secs_f64(8.0));
    assert_eq!(b.timeline_range.end, TimelinePosition::from_secs_f64(15.0));
}

#[test]
fn test_move_clip_splits_target() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Clip A [0, 3) (small), Clip B [5, 20) (big)
    let clip_a = make_clip(asset_id, 0.0, 3.0);
    let clip_a_id = clip_a.id;
    timeline.add_clip(0, clip_a).unwrap();
    timeline
        .add_clip(0, make_clip(asset_id, 5.0, 15.0))
        .unwrap();

    // Move A into the middle of B → [10, 13). Should split B into [5,10) and [13,20)
    timeline
        .move_clip(0, clip_a_id, 0, TimelinePosition::from_secs_f64(10.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 3);

    let b_left = &timeline.tracks[0].clips[0];
    assert_eq!(b_left.timeline_range.start, TimelinePosition::from_secs_f64(5.0));
    assert_eq!(b_left.timeline_range.end, TimelinePosition::from_secs_f64(10.0));

    let moved = timeline.tracks[0].get_clip(clip_a_id).unwrap();
    assert_eq!(moved.timeline_range.start, TimelinePosition::from_secs_f64(10.0));
    assert_eq!(moved.timeline_range.end, TimelinePosition::from_secs_f64(13.0));

    let b_right = &timeline.tracks[0].clips[2];
    assert_eq!(b_right.timeline_range.start, TimelinePosition::from_secs_f64(13.0));
    assert_eq!(b_right.timeline_range.end, TimelinePosition::from_secs_f64(20.0));
}

#[test]
fn test_move_clip_engulfs_smaller() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Clip A [0, 10) (big), Clip B [15, 18) (small)
    let clip_a = make_clip(asset_id, 0.0, 10.0);
    let clip_a_id = clip_a.id;
    timeline.add_clip(0, clip_a).unwrap();
    timeline
        .add_clip(0, make_clip(asset_id, 15.0, 3.0))
        .unwrap();

    // Move A to [14, 24) — fully covers B → B should be removed
    timeline
        .move_clip(0, clip_a_id, 0, TimelinePosition::from_secs_f64(14.0))
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 1);
    let moved = timeline.tracks[0].get_clip(clip_a_id).unwrap();
    assert_eq!(moved.timeline_range.start, TimelinePosition::from_secs_f64(14.0));
    assert_eq!(moved.timeline_range.end, TimelinePosition::from_secs_f64(24.0));
}

#[test]
fn test_time_range_overlaps() {
    let a = TimeRange::new(
        TimelinePosition::from_secs_f64(0.0),
        TimelinePosition::from_secs_f64(5.0),
    )
    .unwrap();

    let b = TimeRange::new(
        TimelinePosition::from_secs_f64(3.0),
        TimelinePosition::from_secs_f64(8.0),
    )
    .unwrap();

    let c = TimeRange::new(
        TimelinePosition::from_secs_f64(5.0),
        TimelinePosition::from_secs_f64(8.0),
    )
    .unwrap();

    assert!(a.overlaps(&b)); // overlapping
    assert!(!a.overlaps(&c)); // adjacent, not overlapping
}

#[test]
fn test_preview_trim_left() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [0, 10)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 10.0))
        .unwrap();

    // Preview drop [5, 15) → should trim existing to [0, 5)
    let previews = timeline.tracks[0].preview_trim_overlaps(5.0, 15.0, None);
    assert_eq!(previews.len(), 1);
    let p = &previews[0];
    assert!((p.original_start - 0.0).abs() < 0.001);
    assert!((p.original_end - 10.0).abs() < 0.001);
    assert!((p.trimmed_start.unwrap() - 0.0).abs() < 0.001);
    assert!((p.trimmed_end.unwrap() - 5.0).abs() < 0.001);
}

#[test]
fn test_preview_trim_right() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [5, 15)
    timeline
        .add_clip(0, make_clip(asset_id, 5.0, 10.0))
        .unwrap();

    // Preview drop [0, 8) → should trim existing to [8, 15)
    let previews = timeline.tracks[0].preview_trim_overlaps(0.0, 8.0, None);
    assert_eq!(previews.len(), 1);
    let p = &previews[0];
    assert!((p.trimmed_start.unwrap() - 8.0).abs() < 0.001);
    assert!((p.trimmed_end.unwrap() - 15.0).abs() < 0.001);
}

#[test]
fn test_preview_full_cover() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [3, 7)
    timeline
        .add_clip(0, make_clip(asset_id, 3.0, 4.0))
        .unwrap();

    // Preview drop [0, 10) → fully covers, should be removed
    let previews = timeline.tracks[0].preview_trim_overlaps(0.0, 10.0, None);
    assert_eq!(previews.len(), 1);
    let p = &previews[0];
    assert!(p.trimmed_start.is_none());
    assert!(p.trimmed_end.is_none());
}

#[test]
fn test_preview_split() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [0, 20)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 20.0))
        .unwrap();

    // Preview drop [5, 10) → split into [0,5) and [10,20)
    let previews = timeline.tracks[0].preview_trim_overlaps(5.0, 10.0, None);
    assert_eq!(previews.len(), 2);

    let left = &previews[0];
    assert!((left.trimmed_start.unwrap() - 0.0).abs() < 0.001);
    assert!((left.trimmed_end.unwrap() - 5.0).abs() < 0.001);

    let right = &previews[1];
    assert!((right.trimmed_start.unwrap() - 10.0).abs() < 0.001);
    assert!((right.trimmed_end.unwrap() - 20.0).abs() < 0.001);
}

#[test]
fn test_preview_no_overlap() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Existing clip [0, 5)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();

    // Preview drop [10, 15) → no overlap
    let previews = timeline.tracks[0].preview_trim_overlaps(10.0, 15.0, None);
    assert!(previews.is_empty());
}

#[test]
fn test_preview_snap_to_adjacent_end() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Clip A at [0, 5)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();

    // Dragging a 3s clip to position 5.1 (end at 8.1) — close to A's end at 5.0
    let previews = timeline.tracks[0].preview_trim_overlaps(5.1, 8.1, None);
    let snap = timeline.tracks[0].preview_snap_position(5.1, 8.1, None, &previews, 0.2);
    assert!(snap.is_some());
    assert!((snap.unwrap() - 5.0).abs() < 0.001, "should snap start to A's end");
}

#[test]
fn test_preview_snap_to_adjacent_start() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Clip A at [5, 10)
    timeline
        .add_clip(0, make_clip(asset_id, 5.0, 5.0))
        .unwrap();

    // Dragging a 3s clip to position 1.85 (end at 4.85) — close to A's start at 5.0
    let previews = timeline.tracks[0].preview_trim_overlaps(1.85, 4.85, None);
    let snap = timeline.tracks[0].preview_snap_position(1.85, 4.85, None, &previews, 0.2);
    assert!(snap.is_some());
    assert!((snap.unwrap() - 2.0).abs() < 0.001, "should snap end to A's start (start=5.0-3.0=2.0)");
}

#[test]
fn test_preview_snap_no_match() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Clip A at [0, 5)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();

    // Dragging a 3s clip to position 10.0 (end at 13.0) — far from A
    let previews = timeline.tracks[0].preview_trim_overlaps(10.0, 13.0, None);
    let snap = timeline.tracks[0].preview_snap_position(10.0, 13.0, None, &previews, 0.2);
    assert!(snap.is_none());
}

#[test]
fn test_preview_snap_uses_trimmed_edges() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);

    let asset_id = Uuid::new_v4();
    // Clip A at [0, 10)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 10.0))
        .unwrap();

    // Dragging a 3s clip to position 4.0 (end at 7.0) — overlaps A
    // A gets trimmed to [0, 4) and [7, 10)
    let previews = timeline.tracks[0].preview_trim_overlaps(4.0, 7.0, None);
    assert_eq!(previews.len(), 2); // split

    // Now drag to 3.85 (end at 6.85) — trimmed left piece would be [0, 3.85)
    // Snap should find trimmed left piece's end at 3.85 is close to... wait,
    // the snap checks the TRIMMED edges, so left piece end = 3.85, right piece start = 6.85
    // Our clip start 3.85 vs left piece end 3.85 = gap 0 → already snapped
    // Let's try a position that would result in a near-snap
    let _previews2 = timeline.tracks[0].preview_trim_overlaps(3.9, 6.9, None);
    // Left piece: [0, 3.9), right piece: [6.9, 10)
    // Now try snapping a clip at [4.05, 7.05) → trimmed edges [0, 4.05) and [7.05, 10)
    // Our start 4.05 vs trimmed left end 4.05 = gap 0, no snap needed
    // Actually let's test that snap works against trimmed edge of a different scenario:

    // Clip B at [15, 20) also on track
    timeline
        .add_clip(0, make_clip(asset_id, 15.0, 5.0))
        .unwrap();

    // Drag a 3s clip to [6.85, 9.85) — overlaps A, trims A to [0, 6.85)
    // Right piece of A: [9.85, 10) — and clip B at [15, 20)
    // Check snap: our end 9.85 vs trimmed right piece start 9.85 = gap 0
    // Our end 9.85 vs B's start 15 = gap 5.15 > threshold
    // So no snap (already aligned). Good.
    // Let's test: clip at [6.9, 9.9) → A trimmed to [0, 6.9) and [9.9, 10)
    // Our start 6.9 near A-left-end 6.9? gap 0 → already aligned
    // Let's just verify no snap at a position far from edges
    let previews3 = timeline.tracks[0].preview_trim_overlaps(3.0, 6.0, None);
    // A trimmed to [0, 3) and [6, 10)
    let snap = timeline.tracks[0].preview_snap_position(3.0, 6.0, None, &previews3, 0.2);
    // start 3.0 vs trimmed-left end 3.0 → gap 0, so it finds a snap with gap 0
    // end 6.0 vs trimmed-right start 6.0 → gap 0
    // Both are gap 0, so effectively already snapped. Should return Some(3.0).
    assert!(snap.is_some());
    assert!((snap.unwrap() - 3.0).abs() < 0.001);
}

// ===== Grouped operations tests =====

#[test]
fn test_track_type_and_group_id() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    let v = timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    let a = timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    assert_eq!(timeline.tracks[v].track_type, TrackType::Video);
    assert_eq!(timeline.tracks[a].track_type, TrackType::Audio);
    assert_eq!(timeline.tracks[v].group_id, Some(group_id));
    assert_eq!(timeline.tracks[a].group_id, Some(group_id));
}

#[test]
fn test_group_members() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    let v = timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    let a = timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));
    timeline.add_track("Standalone", TrackType::Video);

    let members = timeline.group_members(v);
    assert_eq!(members, vec![v, a]);

    let members = timeline.group_members(a);
    assert_eq!(members, vec![v, a]);

    // Standalone track has no group
    let members = timeline.group_members(2);
    assert_eq!(members, vec![2]);
}

#[test]
fn test_find_paired_audio_track() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    assert_eq!(timeline.find_paired_audio_track(0), Some(1));
    assert_eq!(timeline.find_paired_audio_track(1), None); // audio track has no paired audio
}

#[test]
fn test_add_clip_with_audio() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    ).unwrap();

    let (vid, aud) = timeline.add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range).unwrap();

    // Both tracks have one clip
    assert_eq!(timeline.tracks[0].clips.len(), 1);
    assert_eq!(timeline.tracks[1].clips.len(), 1);

    let video_clip = timeline.tracks[0].get_clip(vid).unwrap();
    let audio_clip = timeline.tracks[1].get_clip(aud).unwrap();

    // Same link_id
    assert!(video_clip.link_id.is_some());
    assert_eq!(video_clip.link_id, audio_clip.link_id);

    // Same position and source range
    assert_eq!(video_clip.timeline_range.start, audio_clip.timeline_range.start);
    assert_eq!(video_clip.timeline_range.end, audio_clip.timeline_range.end);
}

#[test]
fn test_find_linked_clips() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    ).unwrap();

    let (vid, aud) = timeline.add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range).unwrap();

    let link_id = timeline.tracks[0].get_clip(vid).unwrap().link_id.unwrap();
    let linked = timeline.find_linked_clips(link_id);

    assert_eq!(linked.len(), 2);
    assert!(linked.contains(&(0, vid)));
    assert!(linked.contains(&(1, aud)));
}

#[test]
fn test_move_clip_grouped() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    ).unwrap();

    let (vid, aud) = timeline.add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range).unwrap();

    // Move the video clip to 3.0s → audio should also move to 3.0s
    timeline.move_clip_grouped(0, vid, 0, TimelinePosition::from_secs_f64(3.0)).unwrap();

    let video_clip = timeline.tracks[0].get_clip(vid).unwrap();
    let audio_clip = timeline.tracks[1].get_clip(aud).unwrap();

    assert_eq!(video_clip.timeline_range.start, TimelinePosition::from_secs_f64(3.0));
    assert_eq!(video_clip.timeline_range.end, TimelinePosition::from_secs_f64(8.0));
    assert_eq!(audio_clip.timeline_range.start, TimelinePosition::from_secs_f64(3.0));
    assert_eq!(audio_clip.timeline_range.end, TimelinePosition::from_secs_f64(8.0));
}

#[test]
fn test_resize_clip_grouped() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    ).unwrap();

    let (vid, aud) = timeline.add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range).unwrap();

    // Resize video to end at 8.0 → audio should also end at 8.0
    timeline.resize_clip_grouped(0, vid, TimelinePosition::from_secs_f64(8.0)).unwrap();

    let video_clip = timeline.tracks[0].get_clip(vid).unwrap();
    let audio_clip = timeline.tracks[1].get_clip(aud).unwrap();

    assert_eq!(video_clip.timeline_range.end, TimelinePosition::from_secs_f64(8.0));
    assert_eq!(audio_clip.timeline_range.end, TimelinePosition::from_secs_f64(8.0));
}

#[test]
fn test_cut_at_grouped() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(10.0),
    ).unwrap();

    let (_vid, _aud) = timeline.add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range).unwrap();

    let results = timeline.cut_at_grouped(0, TimelinePosition::from_secs_f64(4.0)).unwrap();

    // Should have cut both tracks
    assert_eq!(results.len(), 2);
    assert_eq!(timeline.tracks[0].clips.len(), 2);
    assert_eq!(timeline.tracks[1].clips.len(), 2);

    // Left clips should have matching link_ids
    let (v_left, v_right) = results[0];
    let (a_left, a_right) = results[1];

    let v_left_clip = timeline.tracks[0].get_clip(v_left).unwrap();
    let a_left_clip = timeline.tracks[1].get_clip(a_left).unwrap();
    assert_eq!(v_left_clip.link_id, a_left_clip.link_id);
    assert!(v_left_clip.link_id.is_some());

    // Right clips should have matching link_ids (different from left)
    let v_right_clip = timeline.tracks[0].get_clip(v_right).unwrap();
    let a_right_clip = timeline.tracks[1].get_clip(a_right).unwrap();
    assert_eq!(v_right_clip.link_id, a_right_clip.link_id);
    assert!(v_right_clip.link_id.is_some());
    assert_ne!(v_left_clip.link_id, v_right_clip.link_id);

    // Verify cut positions
    assert_eq!(v_left_clip.timeline_range.end, TimelinePosition::from_secs_f64(4.0));
    assert_eq!(v_right_clip.timeline_range.start, TimelinePosition::from_secs_f64(4.0));
    assert_eq!(a_left_clip.timeline_range.end, TimelinePosition::from_secs_f64(4.0));
    assert_eq!(a_right_clip.timeline_range.start, TimelinePosition::from_secs_f64(4.0));
}

#[test]
fn test_grouped_move_with_overlap_trimming() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    ).unwrap();

    // Add linked pair at 0s
    let (vid, aud) = timeline.add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range).unwrap();

    // Add another clip on video track at 8s (will overlap after move)
    let blocker = make_clip(asset_id, 8.0, 5.0);
    timeline.add_clip(0, blocker).unwrap();
    // Add another clip on audio track at 8s
    let audio_blocker = make_clip(asset_id, 8.0, 5.0);
    timeline.add_clip(1, audio_blocker).unwrap();

    // Move linked pair to 7s → [7, 12) overlaps [8, 13) on both tracks → should trim
    timeline.move_clip_grouped(0, vid, 0, TimelinePosition::from_secs_f64(7.0)).unwrap();

    let video_clip = timeline.tracks[0].get_clip(vid).unwrap();
    let audio_clip = timeline.tracks[1].get_clip(aud).unwrap();

    assert_eq!(video_clip.timeline_range.start, TimelinePosition::from_secs_f64(7.0));
    assert_eq!(audio_clip.timeline_range.start, TimelinePosition::from_secs_f64(7.0));
}

#[test]
fn test_track_type_serialization_roundtrip() {
    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    ).unwrap();

    timeline.add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range).unwrap();

    let json = serde_json::to_string(&timeline).unwrap();
    let restored: Timeline = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.tracks[0].track_type, TrackType::Video);
    assert_eq!(restored.tracks[1].track_type, TrackType::Audio);
    assert_eq!(restored.tracks[0].group_id, Some(group_id));
    assert_eq!(restored.tracks[1].group_id, Some(group_id));

    let vid_clip = &restored.tracks[0].clips[0];
    let aud_clip = &restored.tracks[1].clips[0];
    assert!(vid_clip.link_id.is_some());
    assert_eq!(vid_clip.link_id, aud_clip.link_id);
}

#[test]
fn test_undo_redo_grouped_operations() {
    use zeditor_core::commands::CommandHistory;

    let mut timeline = Timeline::new();
    let group_id = uuid::Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));
    let mut history = CommandHistory::new();

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    ).unwrap();

    // Add linked clips via command history
    history.execute(&mut timeline, "Add linked clips", |tl| {
        tl.add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range)
    }).unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 1);
    assert_eq!(timeline.tracks[1].clips.len(), 1);

    // Undo
    history.undo(&mut timeline).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 0);
    assert_eq!(timeline.tracks[1].clips.len(), 0);

    // Redo
    history.redo(&mut timeline).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 1);
    assert_eq!(timeline.tracks[1].clips.len(), 1);
}

#[test]
fn test_split_by_overlap_mirrors_on_linked_track() {
    // When a linked clip is split by add_clip_trimming_overlaps (dragging a
    // smaller clip into the middle of a larger one), the linked audio clip
    // should also be split at the same boundaries. Left pieces keep the
    // original link_id, right pieces get a new link_id, and the middle audio
    // piece (with no video partner) loses its link_id.
    let mut timeline = Timeline::new();
    let group_id = Uuid::new_v4();
    timeline.add_track_with_group("Video 1", TrackType::Video, Some(group_id));
    timeline.add_track_with_group("Audio 1", TrackType::Audio, Some(group_id));

    let asset_id = Uuid::new_v4();
    let source_range = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(20.0),
    };
    // Add linked clips at [0, 20)
    timeline
        .add_clip_with_audio(0, 1, asset_id, TimelinePosition::zero(), source_range)
        .unwrap();

    let original_link_id = timeline.tracks[0].clips[0].link_id;
    assert!(original_link_id.is_some(), "clips should be linked before split");

    // Drop a new clip at [5, 10) on the video track, splitting the linked clip
    let new_clip = make_clip(Uuid::new_v4(), 5.0, 5.0);
    timeline.add_clip_trimming_overlaps(0, new_clip).unwrap();

    // Video track: [left_v 0-5), [new 5-10), [right_v 10-20)
    assert_eq!(timeline.tracks[0].clips.len(), 3);
    let left_v = &timeline.tracks[0].clips[0];
    let middle_v = &timeline.tracks[0].clips[1];
    let right_v = &timeline.tracks[0].clips[2];

    assert_eq!(left_v.timeline_range.start, TimelinePosition::zero());
    assert_eq!(left_v.timeline_range.end, TimelinePosition::from_secs_f64(5.0));
    assert_eq!(right_v.timeline_range.start, TimelinePosition::from_secs_f64(10.0));
    assert_eq!(right_v.timeline_range.end, TimelinePosition::from_secs_f64(20.0));

    // Audio track should also be split: [left_a 0-5), [mid_a 5-10), [right_a 10-20)
    assert_eq!(
        timeline.tracks[1].clips.len(), 3,
        "audio track should have 3 clips after mirrored split"
    );
    let left_a = &timeline.tracks[1].clips[0];
    let mid_a = &timeline.tracks[1].clips[1];
    let right_a = &timeline.tracks[1].clips[2];

    assert_eq!(left_a.timeline_range.start, TimelinePosition::zero());
    assert_eq!(left_a.timeline_range.end, TimelinePosition::from_secs_f64(5.0));
    assert_eq!(mid_a.timeline_range.start, TimelinePosition::from_secs_f64(5.0));
    assert_eq!(mid_a.timeline_range.end, TimelinePosition::from_secs_f64(10.0));
    assert_eq!(right_a.timeline_range.start, TimelinePosition::from_secs_f64(10.0));
    assert_eq!(right_a.timeline_range.end, TimelinePosition::from_secs_f64(20.0));

    // Left video + left audio share the original link_id
    assert_eq!(left_v.link_id, original_link_id);
    assert_eq!(left_a.link_id, original_link_id);

    // Right video + right audio share a NEW link_id (different from original)
    assert!(right_v.link_id.is_some(), "right video should have link_id");
    assert_eq!(right_v.link_id, right_a.link_id, "right video and audio should share link_id");
    assert_ne!(right_v.link_id, original_link_id, "right pieces should have new link_id");

    // Middle audio has no video partner → no link
    assert!(mid_a.link_id.is_none(), "middle audio (no video partner) should have no link_id");

    // New clip has no link
    assert!(middle_v.link_id.is_none(), "new clip should have no link_id");

    // Verify groups work: dragging left_v should only find left_a, not right_v
    let left_linked = timeline.find_linked_clips(original_link_id.unwrap());
    assert_eq!(left_linked.len(), 2, "left group should have exactly 2 clips");
    let right_linked = timeline.find_linked_clips(right_v.link_id.unwrap());
    assert_eq!(right_linked.len(), 2, "right group should have exactly 2 clips");
}
