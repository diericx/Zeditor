# Rendering Profiling and Feedback

We are currently having some render runtime issues (taking too long) and what I would first like us to do is get some solid data on this issue.

Implement profiling that will give us insight into what areas of the render pipeline are taking the longest.

At a high level at the bottom of the screen while rendering we want to see how long the render has taken, what frame we are on out of the total frames, percentage complete, and when it is done show time it took to render.

At a lower level produce some granular metrics about how much time was spent in total on each stage of the render pipeline.

If you can, and if you think this would help, record this info per frame so we can see a line graph over time of the performance and potentially identify moments in the timeline that are causing slowdowns.

Produce this report however you see best but it should be at some level, or some planned way, human readable.

Document how I can produce charts and view averages of these reports.

Enable profiling with an environment variable, default to off.

Default profile file location is next to render file but can be set to a static folder with an env variable.

---

## Execution Log

### Implementation (2026-02-15)

**Files created:**
- `crates/zeditor-media/src/render_profile.rs` — New module with profiling types: `RenderProgress`, `RenderStage`, `RenderProfile`, `ProfileConfig`, `StageTimings`, `FrameMetrics`, `ProfileCollector`. Includes env var helpers (`is_profiling_enabled`, `profile_output_path`) and JSON serialization (`write_profile`).
- `crates/zeditor-media/tests/render_profile_tests.rs` — 9 unit tests covering: disabled/enabled collector, output path (default + env override), serialization roundtrip, file write, env var detection, disabled collector ignoring ops.

**Files modified:**
- `crates/zeditor-media/Cargo.toml` — Added `serde` and `serde_json` workspace deps (both regular and dev).
- `crates/zeditor-media/src/lib.rs` — Added `pub mod render_profile;`.
- `crates/zeditor-media/src/renderer.rs` — Changed `render_timeline` signature to accept `progress_tx: Option<mpsc::Sender<RenderProgress>>`. Added `ProfileCollector` instrumentation: stage timing (setup, video encode, audio encode, flush, write trailer) and per-frame metrics (find_clips, decode, effects, composite, color_convert, encode). Sends progress every 10 frames. Writes `.profile.json` on completion when `ZEDITOR_PROFILE=1`.
- `crates/zeditor-media/tests/renderer_tests.rs` — Updated all 19 existing `render_timeline` calls with `None` as 4th arg. Added 3 integration tests: `test_render_with_profiling_creates_json`, `test_render_without_profiling_no_json`, `test_render_progress_channel_receives_updates`.
- `crates/zeditor-ui/src/app.rs` — Added render state fields (`is_rendering`, `render_progress_rx`, `render_current_frame`, `render_total_frames`, `render_elapsed`, `render_start`). Created progress channel in `RenderFileDialogResult`. Poll progress in `PlaybackTick`. Show total time in `RenderComplete`. Clean up state in `RenderComplete`/`RenderError`. Enhanced status bar with yellow progress text + green progress bar during rendering.
- `README.md` — Added "Render Profiling" section with env var table, usage, profile format, jq quick analysis commands, and Python matplotlib visualization.

**Test results:** 374 tests passing (all workspace tests).

**Key design decisions:**
- Profiling is zero-overhead when disabled: all timing is gated behind `profiler.is_enabled()`.
- Progress uses `mpsc::channel` (unbounded) to avoid blocking the render thread.
- Progress is polled in `PlaybackTick` via `try_recv()` loop.
- Profile stats computed: avg, median, p95, max frame time, slowest frame index.
