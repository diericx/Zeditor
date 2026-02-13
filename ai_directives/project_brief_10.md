# Rendering

We now want to implement the final core piece of our app; video rendering.

- Create a new menu item File -> Render
- By default just render the timeline in h264 in an mkv container with the superfast preset using CRF 22.
- Places with no clip should render black
- Places with a clip should render that clips video and audio
- Render until the last clip then end. In the future we will want to implement adjustable start and end render timecodes but for now default to the end of the last clip in the timeline

---

## Execution Log

### Implementation Summary (Brief 10)

**Files created:**
- `crates/zeditor-media/src/renderer.rs` — Core rendering engine with `RenderConfig`, `derive_render_config()`, and `render_timeline()`. Uses rsmpeg native APIs (libx264 + AAC) for frame-by-frame encoding. Key features:
  - Walks timeline frame-by-frame, decoding source clips via cached `FfmpegDecoder`/`FfmpegAudioDecoder`
  - RGB24→YUV420P conversion via SwsContext for video frames
  - Interleaved f32→FLTP conversion for AAC audio encoding
  - Black frames for gaps, silence for audio gaps
  - Decoder caching with seek optimization (reuses decoders per source path)
  - Derives output resolution/fps from first video clip's source asset, falls back to 1920x1080@30fps
- `crates/zeditor-media/tests/renderer_tests.rs` — 8 integration tests: single clip, gap, multiple clips, audio, empty timeline, config defaults, derive config from asset, derive config empty timeline

**Files modified:**
- `crates/zeditor-media/src/lib.rs` — Added `pub mod renderer`
- `crates/zeditor-ui/src/message.rs` — Added `Render` to `MenuAction`, added `RenderFileDialogResult(Option<PathBuf>)`, `RenderComplete(PathBuf)`, `RenderError(String)` message variants
- `crates/zeditor-ui/src/app.rs` — Added "Render" to File menu dropdown, wired `MenuAction::Render` → save file dialog (MKV filter), `RenderFileDialogResult` → background `Task::perform` with `render_timeline()`, `RenderComplete` → status update, `RenderError` → error status
- `crates/zeditor-ui/tests/message_tests.rs` — 4 new tests: render_complete_sets_status, render_error_sets_status, render_dialog_none_cancels, menu_render_dispatches
- `crates/zeditor-ui/tests/simulator_tests.rs` — 1 new test: file_menu_shows_render

**Key design decisions:**
- Used rsmpeg native encoding (not CLI subprocess) for proper multi-clip timeline rendering with frame-level control
- Video and audio encoded in separate passes (video first, then audio) with `interleaved_write_frame` for proper muxing
- AVStreamMut borrow conflicts resolved by scoping stream creation in blocks (drop before next borrow)
- AVDictionary `set()` returns new dict (consumed + returned pattern), chained for preset+CRF options
- Render runs on a background `Task::perform` thread via iced's async runtime — UI shows "Rendering..." status and updates on completion/error

**Test results:** All 216 workspace tests pass (55 core + 21 media + 2 test-harness + 147 UI including 8 new renderer tests + 5 new UI tests)
