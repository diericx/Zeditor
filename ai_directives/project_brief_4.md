# Refining the current timeline features

The timeline is now playing back video well and clips can be moved around. Lets refine it a bit.

REMEMBER: Write tests for as much of this as you can, even the playback bugs.

- Scrolling alone in the timeline should move left and right
- Scrolling while holding left-alt should zoom in and out
- Do not let me drag a clip to the left of the 0 time mark
- BUG: Clicking play (space bar) while the playback cursor is BEFORE the first clip (say at second 5 but the first clip has been placed at second 30) shows the last frame displayed rather than what should or would be displayed here (which is black frames)
  - If the timeline is played when there is no content, show black as that is what would be rendered.
- BUG: when the playback cursor reaches the end of a clip and there is no clip after it it playback stops
  - IT should continue playback but show black
- BUG: When the playback cursor reaches the end of a clip and there IS a clip after it, it simply stop on the last frame of the ended clip
  - It should transition into playing the next clip
- BUG: clicking to move the playback cursor WHILE playing freezes video on the current frame, but playback cursor and timer keep moving where they were without moving the cursor to where I just clicked
  - It should pause playback, move the cursor to the clicked location and show that frame on screen and stay paused allowing me to play from there if I want.

---

## Implementation Log

### Changes Made

**1. Click during playback pauses (Bug 7)**
- `app.rs` `TimelineClickEmpty` handler: now sets `is_playing = false`, clears `playback_start_wall`, and seeks with `continuous=false` (scrub mode).

**2. Show black when no clip at position (Bugs 4, 5, 6)**
- `app.rs` `send_decode_seek()` no-clip branch: now clears `current_frame`, `pending_frame`, and calls `send_decode_stop()`. This handles playing before first clip, gaps between clips, and clip-to-clip transitions.

**3. Scroll = Pan, Alt+Scroll = Zoom**
- `timeline_canvas.rs`: Added `modifiers: iced::keyboard::Modifiers` to `TimelineCanvasState`.
- Added `ModifiersChanged` handler to track modifier key state.
- Rewrote `WheelScrolled` handler: plain scroll pans horizontally, Alt+scroll zooms centered on cursor.

**4. Test helper getter**
- `test_helpers.rs`: Added `App::decode_clip_id()` getter for integration test assertions.

### New Tests (5 total, 92 total workspace tests)

| Test | File | Verifies |
|------|------|----------|
| `test_click_during_playback_pauses_and_moves_cursor` | `message_tests.rs` | Bug 7 fix: click pauses, moves cursor |
| `test_play_before_first_clip_shows_no_frame` | `playback_tests.rs` | Bug 4: black frame before first clip |
| `test_playback_continues_through_gap_showing_black` | `playback_tests.rs` | Bug 5: continues through gap, shows black |
| `test_clip_transition_triggers_new_decode` | `playback_tests.rs` | Bug 6: transitions decode to next clip |
| `test_drag_position_clamped_to_zero` | `timeline_canvas.rs` | Drag clamp guard at 0s |
