# V1

Implement the first versiont that has the following features

- Area on left of screen is for source clip management, to the right of that is the video viewer. Along the bottom is the timeline.
- user can
  - load files in from the computer as source clips
  - drag source clips onto the timeline from the clip manager
  - zoom in and out of the timeline
  - scroll right and left in the timeline
  - drag clips left and right in the timeline
  - click on a non-clip area of the timeline to move the cursor
  - click play (or space bar) to play the arranged timeline from where the cursor is
  - drag the end of a clip to change its size

---

## Implementation Log

### Files Created
- `crates/zeditor-ui/src/main.rs` - Binary entry point using `iced::application`
- `crates/zeditor-ui/src/widgets/timeline_canvas.rs` - Canvas-based timeline with `canvas::Program`

### Files Modified
- `crates/zeditor-ui/Cargo.toml` - Added `rfd`, `image`/`tokio` features, `[[bin]]` section
- `crates/zeditor-ui/src/message.rs` - Added 8 new message variants (OpenFileDialog, FileDialogResult, SelectSourceAsset, TimelineClickEmpty, PlaceSelectedClip, TogglePlayback, PlaybackTick, FrameDecoded, KeyboardEvent)
- `crates/zeditor-ui/src/app.rs` - Major overhaul: 3-panel layout, file dialog, asset selection, video viewport with frame decode, playback engine with wall-clock timer, keyboard handling, auto-scroll
- `crates/zeditor-ui/src/widgets/mod.rs` - Added `pub mod timeline_canvas`
- `crates/zeditor-ui/tests/message_tests.rs` - Expanded from 11 to 26 tests
- `crates/zeditor-ui/tests/simulator_tests.rs` - Updated for new layout, added 4 new tests (14 total)
- `crates/zeditor-ui/tests/cross_crate_tests.rs` - Added decode pipeline test (4 total)

### Features Implemented
1. Three-panel layout: source library (left), video viewer (right), timeline canvas (bottom)
2. File import via native dialog (`rfd`), probes media with FFmpeg
3. Select-then-place workflow: select asset in source panel, click timeline to place
4. Canvas timeline with time ruler, colored clips, playhead, track lanes
5. Zoom/scroll support on timeline canvas
6. Drag clips to move (with snap-to-adjacent), drag right edge to resize
7. Click empty area to move cursor (playhead)
8. Spacebar toggles play/pause, wall-clock-based playback with 33ms tick
9. Video frame decoding at playhead position (RGB24→RGBA32 conversion)
10. Auto-scroll during playback
11. Status bar with HH:MM:SS.mmm, zoom level, playback state

### Test Results
- 81 tests pass (`cargo test --workspace`)
- 6 canvas unit tests (coordinate math, hit testing, zoom clamping)
- 26 message-level tests (all handlers including new ones)
- 14 simulator UI tests (layout, buttons, selection, import)
- 4 cross-crate integration tests (probe, decode, RGBA conversion)

### Architecture Notes
- Canvas `Program` takes `&'a Timeline` reference - no lifetime issues
- Frame decode: opens/closes decoder per request (V1 simplicity, future: cache decoder)
- Playback uses wall-clock `Instant` for accurate timing independent of tick rate
- `rfd` 0.15 with async file dialog, `tokio` feature on iced for timer subscription
- `keyboard::listen()` subscription maps all keyboard events through `KeyboardEvent` message
