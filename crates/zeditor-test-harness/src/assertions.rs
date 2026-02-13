use zeditor_core::timeline::{Timeline, TimelinePosition};

/// Assert that a track has a specific number of clips.
pub fn assert_track_clip_count(timeline: &Timeline, track_index: usize, expected: usize) {
    let track = &timeline.tracks[track_index];
    assert_eq!(
        track.clips.len(),
        expected,
        "track {} has {} clips, expected {}",
        track_index,
        track.clips.len(),
        expected
    );
}

/// Assert that no clips overlap on a given track.
pub fn assert_no_overlaps(timeline: &Timeline, track_index: usize) {
    let track = &timeline.tracks[track_index];
    for (i, a) in track.clips.iter().enumerate() {
        for b in track.clips.iter().skip(i + 1) {
            assert!(
                !a.timeline_range.overlaps(&b.timeline_range),
                "clips {:?} and {:?} overlap on track {}",
                a.id,
                b.id,
                track_index
            );
        }
    }
}

/// Assert that clips are sorted by start position on a track.
pub fn assert_clips_sorted(timeline: &Timeline, track_index: usize) {
    let track = &timeline.tracks[track_index];
    for window in track.clips.windows(2) {
        assert!(
            window[0].timeline_range.start <= window[1].timeline_range.start,
            "clips not sorted on track {}: {:?} should come before {:?}",
            track_index,
            window[0].id,
            window[1].id
        );
    }
}

/// Assert the timeline total duration is approximately the expected value.
pub fn assert_timeline_duration_approx(
    timeline: &Timeline,
    expected_secs: f64,
    tolerance_secs: f64,
) {
    let actual = timeline.duration().as_secs_f64();
    assert!(
        (actual - expected_secs).abs() < tolerance_secs,
        "timeline duration {actual:.3}s != expected {expected_secs:.3}s (tolerance {tolerance_secs:.3}s)"
    );
}

/// Assert that a clip exists at the given position on a track.
pub fn assert_clip_at(timeline: &Timeline, track_index: usize, position_secs: f64) {
    let track = &timeline.tracks[track_index];
    let pos = TimelinePosition::from_secs_f64(position_secs);
    assert!(
        track.clip_at(pos).is_some(),
        "expected clip at {position_secs}s on track {track_index}, but none found"
    );
}
