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

### Bug Fix: Scrambled Video, Choppy Audio, Wrong Resolution

**Root causes identified:**
1. **Video scramble (diagonal lines)**: `FfmpegDecoder::frame_to_scaled()` extracted RGB data with `from_raw_parts(data[0], width*height*bpp)` which didn't account for linesize padding. When `linesize > width*bpp` (due to 32-byte alignment), rows were shifted producing diagonal scrambled lines. The renderer then re-created an AVFrame from this corrupted data via `rgb_to_yuv()`, propagating the corruption.
2. **Wrong resolution**: `derive_render_config()` correctly picks up source asset dimensions, but the stride-corrupted video data made the output appear distorted. The actual encoder dimensions were correct.
3. **Choppy audio**: `encode_audio_frames()` called `decode_and_create_audio_frame()` per output frame, which sought and decoded per-frame. This caused gaps between decoded frames (each seek lands on a keyframe, skipping intermediate samples) producing choppy/stuttering audio.

**Fix approach:**
1. **Video**: Added `decode_next_raw_frame()` to `FfmpegDecoder` — returns raw `AVFrame` in the decoder's native pixel format (typically YUV420P for h264) without any RGB conversion. The renderer now uses per-source `SwsContext` to convert directly from source pixel format → YUV420P at target dimensions, completely bypassing the RGB round-trip and its stride bug.
2. **Audio**: Rewrote audio encoding as "offline clip-at-a-time" rendering:
   - Pre-allocates a contiguous f32 sample buffer for the entire timeline duration at 48kHz stereo
   - Processes each audio clip independently: opens source, creates `SwrContext` for format+rate conversion to 48kHz stereo f32, seeks to clip start, decodes sequentially, writes samples at the correct buffer position
   - Slices the buffer into AAC frame-sized chunks and encodes
   - Handles sample rate conversion (e.g. 44100→48000) via SwrContext
   - No per-frame seeking — fully sequential decode within each clip

**Files modified:**
- `crates/zeditor-media/src/decoder.rs` — Added `pub(crate) fn decode_next_raw_frame()` returning `(AVFrame, f64)` and `fn frame_pts_secs()` helper
- `crates/zeditor-media/src/renderer.rs` — Major rewrite:
  - `CachedVideoDecoder` now has per-source `sws_ctx: Option<SwsContext>`
  - `decode_and_convert_video_frame()` uses raw frames + direct SWS (removed `rgb_to_yuv()`)
  - Removed global SWS context from `encode_video_frames()` parameters
  - Replaced `encode_audio_frames()` with `encode_audio_offline()` (clip-at-a-time approach)
  - Added `decode_audio_clip_into_buffer()` for per-clip sequential decode with SwrContext resampling
  - Added `convert_audio_frame()` and `write_samples_to_buffer()` helpers
  - Removed `FfmpegAudioDecoder` dependency (uses raw rsmpeg audio decode in renderer)

**Test results:** All 216 workspace tests pass

### Fix Default Render Resolution & Improve Scaling Quality

**Problem:** `derive_render_config()` overrode the 1920x1080 default with the source clip's dimensions (e.g. 1744x1308), causing renders at unexpected resolutions. Additionally, the renderer used `SWS_FAST_BILINEAR` (speed-optimized) for scaling, whereas final render output should prioritize quality.

**Changes to `crates/zeditor-media/src/renderer.rs`:**
- Added `ScalingAlgorithm` enum with `FastBilinear`, `Bilinear`, `Bicubic`, `Lanczos` variants, each mapping to the corresponding `ffi::SWS_*` flag
- Extended `RenderConfig` with `pub scaling: ScalingAlgorithm` field (default: `Lanczos`)
- Fixed `derive_render_config()` — removed resolution override (`config.width = asset.width`, `config.height = asset.height`). Resolution always stays at default 1920x1080. FPS still derived from source to avoid temporal artifacts.
- Threaded `sws_flags` through `encode_video_frames()` → `decode_and_convert_video_frame()` so the configurable scaling algorithm is used instead of hardcoded `SWS_FAST_BILINEAR`
- Added doc comment listing future extensibility fields (video_codec, audio_codec, container_format, etc.)

**Changes to `crates/zeditor-media/tests/renderer_tests.rs`:**
- Updated import to include `ScalingAlgorithm`
- Added `scaling: ScalingAlgorithm::Lanczos` to all 4 manual `RenderConfig` struct literals
- Added `scaling` assertion to `test_render_config_defaults`
- Fixed `test_derive_render_config_from_asset` — assertions changed from 320x240 → 1920x1080 (was testing buggy behavior), added FPS assertion
- Added 3 new e2e tests:
  - `test_render_upscale_to_1080p` — 320x240 source rendered at 1920x1080, verifies output dimensions and duration
  - `test_render_upscale_with_audio` — same with audio track, verifies 1920x1080 + audio present
  - `test_derive_render_config_preserves_1080p_with_any_source` — verifies derive always returns 1920x1080 regardless of source

**Test results:** All 219 workspace tests pass (55 core + 24 media + 2 test-harness + 138 UI)
