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
    timeline.add_track("Video 1");

    let asset_id = Uuid::new_v4();
    let clip = make_clip(asset_id, 0.0, 5.0);

    timeline.add_clip(0, clip).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 1);
}

#[test]
fn test_add_multiple_clips_no_overlap() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");
    timeline.add_track("Video 2");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");
    timeline.add_track("Video 2");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

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
    timeline.add_track("Video 1");

    let asset_id = Uuid::new_v4();
    // Existing clip [0, 5)
    timeline
        .add_clip(0, make_clip(asset_id, 0.0, 5.0))
        .unwrap();

    // Preview drop [10, 15) → no overlap
    let previews = timeline.tracks[0].preview_trim_overlaps(10.0, 15.0, None);
    assert!(previews.is_empty());
}
