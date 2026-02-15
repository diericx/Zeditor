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
