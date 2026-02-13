#![allow(unused_must_use)]

use std::path::PathBuf;
use std::time::Duration;

use uuid::Uuid;
use zeditor_core::media::MediaAsset;
use zeditor_core::timeline::TimelinePosition;
use zeditor_ui::app::App;
use zeditor_ui::message::Message;
use zeditor_ui::test_helpers::{TestFrame, TestFrameSender};

fn make_test_asset(name: &str, duration_secs: f64) -> MediaAsset {
    MediaAsset::new(
        name.into(),
        PathBuf::from(format!("/test/{name}.mp4")),
        Duration::from_secs_f64(duration_secs),
        320,
        240,
        30.0,
        false,
    )
}

/// Create a 2x2 RGBA test frame with a solid color.
fn solid_frame(pts_secs: f64, r: u8, g: u8, b: u8) -> TestFrame {
    let pixel = [r, g, b, 255u8];
    TestFrame {
        rgba: pixel.repeat(4), // 2x2 = 4 pixels
        width: 2,
        height: 2,
        pts_secs,
    }
}

/// Set up an App with a test channel and a clip on the timeline.
/// Returns (app, sender, clip_id).
///
/// The clip's source range starts at 0s with the given duration,
/// placed on the timeline at `clip_tl_start`.
fn setup_app_with_clip(clip_tl_start: f64, duration: f64) -> (App, TestFrameSender, Uuid) {
    let (mut app, sender) = App::new_with_test_channel();

    let asset = make_test_asset("test_clip", duration);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(clip_tl_start),
    });

    let clip_id = app.project.timeline.tracks[0].clips[0].id;

    // Configure decode state to match this clip
    app.set_decode_clip_id(Some(clip_id));
    // offset = clip_tl_start - source_start (source starts at 0)
    app.set_decode_time_offset(clip_tl_start);

    (app, sender, clip_id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_frame_displayed_when_pts_matches_playback() {
    let (mut app, sender, _clip_id) = setup_app_with_clip(0.0, 10.0);

    // Start playing and fake 500ms elapsed
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    app.playback_start_wall = Some(start - Duration::from_millis(500));

    // Inject a frame at PTS 0.5s (maps to timeline 0.5s with offset 0)
    sender.send_frame(solid_frame(0.5, 255, 0, 0));

    // Tick — playback position should be ~0.5s, frame should display
    app.update(Message::PlaybackTick);

    assert!(
        app.current_frame.is_some(),
        "frame at PTS 0.5 should be displayed when playback is at ~0.5s"
    );
}

#[test]
fn test_frame_held_when_ahead_of_playback() {
    let (mut app, sender, _clip_id) = setup_app_with_clip(0.0, 10.0);

    // Start playing with 100ms elapsed
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    app.playback_start_wall = Some(start - Duration::from_millis(100));

    // Inject a frame at PTS 2.0s — well ahead of 0.1s playback
    sender.send_frame(solid_frame(2.0, 0, 255, 0));

    app.update(Message::PlaybackTick);

    // Frame should NOT be displayed yet (PTS 2.0 >> playback ~0.1)
    assert!(
        app.current_frame.is_none(),
        "frame at PTS 2.0 should be held when playback is at ~0.1s"
    );

    // Now advance to 2.0s
    app.playback_start_wall = Some(start - Duration::from_millis(2000));
    app.update(Message::PlaybackTick);

    assert!(
        app.current_frame.is_some(),
        "frame at PTS 2.0 should display when playback catches up to ~2.0s"
    );
}

#[test]
fn test_scrub_frame_shown_immediately_when_paused() {
    let (mut app, sender, _clip_id) = setup_app_with_clip(0.0, 10.0);

    // Stay paused, seek to 3.0s
    app.update(Message::SeekTo(TimelinePosition::from_secs_f64(3.0)));
    assert!(!app.is_playing);

    // Inject a frame at PTS 3.0s
    sender.send_frame(solid_frame(3.0, 0, 0, 255));

    // Tick while paused — should show immediately
    app.update(Message::PlaybackTick);

    assert!(
        app.current_frame.is_some(),
        "scrub frame should display immediately when paused"
    );
}

#[test]
fn test_scrub_shows_frame_even_if_pts_mismatched() {
    let (mut app, sender, _clip_id) = setup_app_with_clip(0.0, 10.0);

    // Paused at 1.0s
    app.update(Message::SeekTo(TimelinePosition::from_secs_f64(1.0)));
    assert!(!app.is_playing);

    // Inject a frame with PTS 5.0s — large mismatch
    sender.send_frame(solid_frame(5.0, 128, 128, 0));

    app.update(Message::PlaybackTick);

    assert!(
        app.current_frame.is_some(),
        "paused mode should display any frame regardless of PTS mismatch"
    );
}

#[test]
fn test_decode_time_offset_maps_source_pts_to_timeline() {
    // Clip starts at timeline 5.0s, source starts at 0.0s
    // So offset = 5.0, and source PTS 1.0 → timeline 6.0
    let (mut app, sender, _clip_id) = setup_app_with_clip(5.0, 10.0);

    // Seek into the clip first so Play finds the clip and preserves decode state
    app.update(Message::SeekTo(TimelinePosition::from_secs_f64(5.0)));
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    // Advance 1s from 5.0 → playback at 6.0
    app.playback_start_wall = Some(start - Duration::from_millis(1000));

    // Inject frame AFTER Play (Play drains the channel)
    sender.send_frame(solid_frame(1.0, 255, 128, 0));

    app.update(Message::PlaybackTick);

    assert!(
        app.current_frame.is_some(),
        "source PTS 1.0 + offset 5.0 = timeline 6.0 should display at playback 6.0"
    );
}

#[test]
fn test_decode_time_offset_gates_future_frames() {
    // Clip at timeline 5.0s, offset = 5.0
    let (mut app, sender, _clip_id) = setup_app_with_clip(5.0, 10.0);

    // Seek into the clip first so Play finds it and preserves decode state
    app.update(Message::SeekTo(TimelinePosition::from_secs_f64(5.0)));
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    // Advance 0.5s from 5.0 → playback at 5.5
    app.playback_start_wall = Some(start - Duration::from_millis(500));

    // Inject frame AFTER Play (Play drains the channel)
    // Source PTS 3.0 + offset 5.0 = timeline 8.0, ahead of 5.5
    sender.send_frame(solid_frame(3.0, 0, 128, 255));

    app.update(Message::PlaybackTick);

    assert!(
        app.current_frame.is_none(),
        "source PTS 3.0 + offset 5.0 = timeline 8.0 should be held at playback 5.5"
    );
}

#[test]
fn test_pending_frame_survives_across_ticks() {
    let (mut app, sender, _clip_id) = setup_app_with_clip(0.0, 10.0);

    // Play with 100ms elapsed
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    app.playback_start_wall = Some(start - Duration::from_millis(100));

    // Inject a frame at PTS 1.0 (ahead of 0.1)
    sender.send_frame(solid_frame(1.0, 64, 64, 64));

    // Tick 1: frame should be held
    app.update(Message::PlaybackTick);
    assert!(
        app.current_frame.is_none(),
        "frame at PTS 1.0 should be held at playback ~0.1s"
    );

    // Tick 2: still at ~0.1s (no wall clock change), frame still held
    app.update(Message::PlaybackTick);
    assert!(
        app.current_frame.is_none(),
        "frame should remain held on second tick"
    );

    // Tick 3: advance to 1.0s
    app.playback_start_wall = Some(start - Duration::from_millis(1000));
    app.update(Message::PlaybackTick);
    assert!(
        app.current_frame.is_some(),
        "frame should finally display when playback reaches 1.0s"
    );
}

#[test]
fn test_e2e_decode_and_playback_sequence() {
    use zeditor_media::decoder::{FfmpegDecoder, VideoDecoder};
    use zeditor_test_harness::fixtures;

    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "e2e_playback", 2.0);

    let (mut app, sender) = App::new_with_test_channel();

    // Import the real asset
    let asset = zeditor_media::probe::probe(&video_path).expect("probe should succeed");
    let asset_id = asset.id;
    let duration = asset.duration.as_secs_f64();
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    let clip_id = app.project.timeline.tracks[0].clips[0].id;
    app.set_decode_clip_id(Some(clip_id));
    app.set_decode_time_offset(0.0);

    // Start playback first (this drains the channel), then inject frames
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    // Advance to 1s — well past the first few frames of a 30fps video
    app.playback_start_wall = Some(start - Duration::from_secs(1));

    // Decode a few real frames and inject them after Play
    let mut decoder = FfmpegDecoder::open(&video_path).expect("decoder open");
    let mut frames_sent = 0;
    let max_frames = 3;

    while frames_sent < max_frames {
        match decoder.decode_next_frame_rgba_scaled(320, 240) {
            Ok(Some(vf)) => {
                sender.send_frame(TestFrame {
                    rgba: vf.data,
                    width: vf.width,
                    height: vf.height,
                    pts_secs: vf.pts_secs,
                });
                frames_sent += 1;
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    assert!(frames_sent > 0, "should have decoded at least one frame");

    app.update(Message::PlaybackTick);

    assert!(
        app.current_frame.is_some(),
        "at least one real decoded frame should be displayed after 1s of playback"
    );
    assert!(
        duration > 1.0,
        "test video should be longer than 1s (got {duration})"
    );
}

// ---------------------------------------------------------------------------
// Playback past end and clip transition tests
// ---------------------------------------------------------------------------

#[test]
fn test_playback_past_last_clip_continues_with_black() {
    let (mut app, _sender) = App::new_with_test_channel();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(0.0),
    });

    let clip_id = app.project.timeline.tracks[0].clips[0].id;
    app.set_decode_clip_id(Some(clip_id));
    app.set_decode_time_offset(0.0);

    // Start playing and advance to 7s (past the 5s clip)
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    app.playback_start_wall = Some(start - Duration::from_secs(7));

    app.update(Message::PlaybackTick);

    // Should still be playing
    assert!(app.is_playing, "playback should continue past last clip");
    assert!(
        app.playback_position.as_secs_f64() > 5.0,
        "position should be past the clip end"
    );
    // No clip at position 7s → frame should be cleared (black)
    assert!(
        app.current_frame.is_none(),
        "should show black (no frame) past last clip"
    );
}

#[test]
fn test_playback_transitions_between_adjacent_clips() {
    let (mut app, _sender) = App::new_with_test_channel();

    let asset1 = make_test_asset("clip_a", 5.0);
    let asset1_id = asset1.id;
    app.update(Message::MediaImported(Ok(asset1)));
    app.update(Message::AddClipToTimeline {
        asset_id: asset1_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(0.0),
    });

    let asset2 = make_test_asset("clip_b", 5.0);
    let asset2_id = asset2.id;
    app.update(Message::MediaImported(Ok(asset2)));
    app.update(Message::AddClipToTimeline {
        asset_id: asset2_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(5.0),
    });

    let clip1_id = app.project.timeline.tracks[0].clips[0].id;
    let clip2_id = app.project.timeline.tracks[0].clips[1].id;

    // Start playing inside clip1
    app.set_decode_clip_id(Some(clip1_id));
    app.set_decode_time_offset(0.0);
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();

    // Advance to 5.5s — should be inside clip2
    app.playback_start_wall = Some(start - Duration::from_millis(5500));
    app.update(Message::PlaybackTick);

    assert_eq!(
        app.decode_clip_id(),
        Some(clip2_id),
        "decode should switch to clip2 after crossing 5s boundary"
    );
    assert!(app.is_playing, "playback should continue into clip2");
}

#[test]
fn test_adjacent_clip_transition_discards_stale_pending() {
    // Reproduces: clip1 [0,5), clip2 [5,10). At transition, a stale clip1 frame
    // races into the channel. With clip2's offset it maps to timeline 10.0 and
    // would be stored as pending, blocking clip2's real frames forever.
    let (mut app, sender) = App::new_with_test_channel();

    let asset1 = make_test_asset("clip_a", 5.0);
    let asset1_id = asset1.id;
    app.update(Message::MediaImported(Ok(asset1)));
    app.update(Message::AddClipToTimeline {
        asset_id: asset1_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(0.0),
    });

    let asset2 = make_test_asset("clip_b", 5.0);
    let asset2_id = asset2.id;
    app.update(Message::MediaImported(Ok(asset2)));
    app.update(Message::AddClipToTimeline {
        asset_id: asset2_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(5.0),
    });

    let clip1_id = app.project.timeline.tracks[0].clips[0].id;

    // Playing inside clip1
    app.set_decode_clip_id(Some(clip1_id));
    app.set_decode_time_offset(0.0);
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();

    // Advance to 5.5s — crosses into clip2
    app.playback_start_wall = Some(start - Duration::from_millis(5500));

    // Inject a stale clip1 frame that will race into the channel.
    // With clip2's offset (5.0), PTS 4.9 → timeline 9.9, way ahead of 5.5s.
    sender.send_frame(solid_frame(4.9, 255, 0, 0));

    // Transition tick: should detect clip2, discard stale frame
    app.update(Message::PlaybackTick);

    // Now inject clip2's first real frame (PTS 0.5, offset 5.0 → timeline 5.5)
    sender.send_frame(solid_frame(0.5, 0, 255, 0));

    // Next tick: should display clip2's frame, not be blocked by stale pending
    app.update(Message::PlaybackTick);

    assert!(
        app.current_frame.is_some(),
        "clip2's frame should be displayed, not blocked by stale pending"
    );
}

#[test]
fn test_stale_frame_after_clip_end_is_discarded() {
    // Reproduces race: decode worker sends a frame after transition clears state.
    let (mut app, sender) = App::new_with_test_channel();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(0.0),
    });

    let clip_id = app.project.timeline.tracks[0].clips[0].id;
    app.set_decode_clip_id(Some(clip_id));
    app.set_decode_time_offset(0.0);

    // Start playing and advance past the clip
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    app.playback_start_wall = Some(start - Duration::from_secs(7));

    // Inject a stale frame BEFORE the tick — simulates the decode worker
    // having queued a frame that arrives after the transition logic runs
    sender.send_frame(solid_frame(4.9, 255, 0, 0));

    // First tick: transition fires, clears state, but poll_decoded_frame
    // must not re-display the stale frame
    app.update(Message::PlaybackTick);
    assert!(
        app.current_frame.is_none(),
        "stale frame should be discarded after clip transition to gap"
    );

    // Inject another stale frame and tick again — should still be black
    sender.send_frame(solid_frame(5.1, 0, 255, 0));
    app.update(Message::PlaybackTick);
    assert!(
        app.current_frame.is_none(),
        "subsequent stale frames should also be discarded"
    );
}

// ---------------------------------------------------------------------------
// Bug fix tests: black frame on gaps, clip transitions
// ---------------------------------------------------------------------------

/// Helper: set up an app with two clips and a gap between them.
/// Clip1: [clip1_start .. clip1_start+dur1], Clip2: [clip2_start .. clip2_start+dur2]
fn setup_two_clips(
    clip1_start: f64,
    dur1: f64,
    clip2_start: f64,
    dur2: f64,
) -> (App, TestFrameSender) {
    let (mut app, sender) = App::new_with_test_channel();

    let asset1 = make_test_asset("clip_a", dur1);
    let asset1_id = asset1.id;
    app.update(Message::MediaImported(Ok(asset1)));
    app.update(Message::AddClipToTimeline {
        asset_id: asset1_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(clip1_start),
    });

    let asset2 = make_test_asset("clip_b", dur2);
    let asset2_id = asset2.id;
    app.update(Message::MediaImported(Ok(asset2)));
    app.update(Message::AddClipToTimeline {
        asset_id: asset2_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(clip2_start),
    });

    (app, sender)
}

#[test]
fn test_play_before_first_clip_shows_no_frame() {
    let (mut app, _sender) = App::new_with_test_channel();

    // Place clip starting at 30s
    let asset = make_test_asset("late_clip", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(30.0),
    });

    // Set a stale frame to prove it gets cleared
    app.set_decode_clip_id(None);
    // Manually set a current_frame to simulate stale state
    // (In real usage this would be leftover from a previous clip)

    // Play at position 0 — no clip here
    app.update(Message::Play);

    assert!(
        app.current_frame.is_none(),
        "should show black (no frame) when playing before first clip"
    );
}

#[test]
fn test_playback_continues_through_gap_showing_black() {
    // Clip1: 0-5s, gap: 5-10s, Clip2: 10-15s
    let (mut app, _sender) = setup_two_clips(0.0, 5.0, 10.0, 5.0);

    let clip1_id = app.project.timeline.tracks[0].clips[0].id;

    // Start playing and advance to 3s (inside clip1)
    app.set_decode_clip_id(Some(clip1_id));
    app.set_decode_time_offset(0.0);
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();
    // Advance to 6s — inside the gap
    app.playback_start_wall = Some(start - Duration::from_secs(6));

    app.update(Message::PlaybackTick);

    // Should still be playing
    assert!(app.is_playing, "playback should continue through gap");
    // current_frame should be None (black) because no clip at 6s
    assert!(
        app.current_frame.is_none(),
        "should show black (no frame) during gap between clips"
    );
}

#[test]
fn test_clip_transition_triggers_new_decode() {
    // Clip1: 0-5s, Clip2: 5-10s (adjacent, no gap)
    let (mut app, _sender) = setup_two_clips(0.0, 5.0, 5.0, 5.0);

    let clip1_id = app.project.timeline.tracks[0].clips[0].id;
    let clip2_id = app.project.timeline.tracks[0].clips[1].id;

    // Start playing inside clip1
    app.set_decode_clip_id(Some(clip1_id));
    app.set_decode_time_offset(0.0);
    app.update(Message::Play);
    let start = app.playback_start_wall.unwrap();

    // Advance past clip1 boundary into clip2 (at 5.5s)
    app.playback_start_wall = Some(start - Duration::from_millis(5500));

    app.update(Message::PlaybackTick);

    // decode_clip_id should have switched to clip2
    assert_eq!(
        app.decode_clip_id(),
        Some(clip2_id),
        "decode should switch to clip2 after crossing boundary"
    );
    assert!(app.is_playing, "playback should continue into next clip");
}
