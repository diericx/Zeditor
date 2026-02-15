# Small bug fixes

- If I begin playback in the middle of a single clip it is fine. If I begin playback in the middle of overlapped clips audio is super choppy.
- Transform offset should be a number field not a slider
- Playback in the editor crashes if you move a clip too far with offset which is easy to do by accident
  - thread '<unnamed>' (2978988) panicked at crates/zeditor-core/src/pipeline.rs:159:24:
    range end index 17179868872 out of range for slice of length 2073600
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrack

---

## Execution Log

### Bug 3: Crash with large transform offset
- **Root cause**: `out_y_end` and `out_x_end` in `TransformEffect::process()` (pipeline.rs:142-144) cast negative i64 values to u32, wrapping to huge values that bypass the bounds check and cause out-of-bounds slice access.
- **Fix**: Added `0i64.max(...)` clamp before the u32 cast on both `out_y_end` and `out_x_end` (pipeline.rs:142-143).
- **Tests added**: 4 tests in `pipeline::tests` — `test_transform_large_negative_offset_no_panic`, `test_transform_large_positive_offset_no_panic`, `test_transform_large_negative_x_only`, `test_transform_large_negative_y_only`.

### Bug 1: Choppy audio with overlapping clips
- **Root cause**: `audio_decode_worker` (app.rs) used `HashMap<PathBuf, CachedAudioDecoder>` — when two clips referenced the same source file (common with overlapping audio), they shared one decoder, causing each clip to consume every other frame.
- **Fix**: Changed to `Vec<Option<CachedAudioDecoder>>` indexed by clip position in `multi_clips`. Added `path: PathBuf` field to `CachedAudioDecoder` to track which file the decoder is for (reused if path matches). Each clip now gets its own independent decoder instance. Also removed unused `target_time` variable from audio worker.

### Bug 2: Transform offset → number field
- **Root cause**: All effect parameters used `slider` widget, but Transform x/y offset ranges ±10,000 making sliders unusable for precise values.
- **Fix**:
  - Added `text_input` to widget imports (app.rs:7).
  - Added `effect_param_texts: HashMap<(Uuid, String), String>` to `App` struct for tracking text input state.
  - Added `EffectParamTextInput` message variant to `message.rs`.
  - In view: parameters with `(max - min) > 100.0` render a `text_input` (width 80, size 12) instead of a slider.
  - In update: `EffectParamTextInput` stores text, and if parseable as f64 within bounds, updates the effect parameter. `UpdateEffectParameter` (from slider) clears any stale text state.
- **Tests added**: 5 tests in `message_tests.rs` — `test_effect_param_text_input_valid_number`, `test_effect_param_text_input_invalid_text`, `test_effect_param_text_input_out_of_bounds`, `test_effect_param_text_input_negative_value`, `test_update_effect_param_clears_text_state`.

### Bug 1 follow-up: Audio change detection (additional fix)
- **Root cause**: The audio change detection in PlaybackTick tracked only a single clip ID (`audio_decode_clip_id: Option<Uuid>`), while the video side correctly tracked all clip IDs in a Vec. This meant entering/leaving overlap zones wasn't properly detected, causing stale decoder state.
- **Fix**:
  - Changed `audio_decode_clip_id: Option<Uuid>` → `audio_decode_clip_ids: Vec<Uuid>` (mirroring video's `decode_clip_ids`)
  - Updated PlaybackTick change detection to use `all_audio_clips_at_position()` and compare the full Vec of clip IDs
  - Updated `send_audio_decode_seek` to store all clip IDs
  - Updated `poll_decoded_audio` to check `is_empty()` instead of `is_none()`
  - Updated test helpers and all 6 playback tests to use the new API
  - Increased audio sync_channel buffer from 4 to 16 frames for better headroom with multi-clip decode

### Bug 1 fix #2: Reverted multi-clip change detection + AVDISCARD_ALL optimization
- **Problem**: The multi-clip change detection (`audio_decode_clip_ids: Vec<Uuid>`) caused a REGRESSION — entering an overlap zone changed the clip ID set, triggering `send_audio_decode_seek(true)` which called `player.clear()`, draining all buffered audio and causing an audible gap. Made things worse than before (stuttering even when starting before the overlap).
- **Root cause of original choppiness**: `FfmpegAudioDecoder::decode_next_audio_frame()` calls `read_packet()` in a loop, reading and discarding ALL video packets to find audio packets. For video files with high-bitrate video, this means reading ~89+ video packets per audio frame. With TWO decoders for overlapping clips, the worker couldn't keep up with real-time.
- **Fix 1 (revert)**: Reverted `audio_decode_clip_ids: Vec<Uuid>` back to `audio_decode_clip_id: Option<Uuid>` (single-clip change detection). Updated PlaybackTick to use `audio_clip_at_position()` instead of `all_audio_clips_at_position()`. Reverted test_helpers.rs and playback_tests.rs.
- **Fix 2 (performance)**: Added `AVDISCARD_ALL` on all non-audio streams in `FfmpegAudioDecoder::open()` (audio_decoder.rs). This tells the FFmpeg demuxer to skip video packets entirely during `read_packet()`, making audio decoding from video files dramatically faster and able to keep up with real-time even with multiple decoders.

### Verification
- All 362 workspace tests pass (0 failures).
- New tests: 4 (pipeline) + 5 (message) = 9 tests added.
