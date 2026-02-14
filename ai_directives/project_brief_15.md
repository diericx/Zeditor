# Functional track layering in editor player AND renders

- When two audio clips overlap, play them both at the same time
- When video clips overlap, render them both but layer them correctly
  - Layer the same way they are visually on the timeline; V1 is on the bottom and VN is on the top
  - If there are two video tracks
    - If V2 fully covers the screen it would be the only thing showing while playing
    - If we then transform V2 to the right for example we would then see part of V1 and part of V2

---

## Execution Log

### Part 1: Renderer Video Compositing (`renderer.rs`)
- Added `find_all_video_clips_at()` — iterates all video tracks in reverse order (V1 first → VN last) to collect every clip at a given position with path, source time, and effects
- Created `decode_and_scale_clip()` — extracted from `decode_and_convert_video_frame()`, decodes a single clip and returns scaled+rotated YUV frame + `CanvasLayout` without blitting onto a canvas
- Created `scale_and_rotate_clip()` — extracted scaling+rotation logic from `compose_clip_onto_canvas_rotated()`
- Refactored `encode_video_frames()` — now calls `find_all_video_clips_at()`, creates a black canvas, then blits each clip bottom-to-top (V1 first, VN last covers V1)
- Old single-clip functions retained with `#[allow(dead_code)]` for potential future use

### Part 2: Renderer Audio Mixing (`renderer.rs`)
- Changed `write_samples_to_buffer()` from `copy_from_slice` (overwrites) to additive mixing: `output[i] = (output[i] + sample[i]).clamp(-1.0, 1.0)`
- Made function `pub` for unit testing
- `encode_audio_offline()` already iterates all audio tracks/clips, so no other changes needed

### Part 3: Preview Video Compositing (`app.rs`)
- Added `ClipDecodeInfo` struct with path, time, and transform fields
- Changed `DecodeRequest` to only have `SeekMulti { clips, continuous, canvas_w, canvas_h }` variant (removed single-clip `Seek`)
- Added `all_video_clips_at_position()` method to App — returns all video clips ordered V1→VN
- Added `decode_clip_ids: Vec<Uuid>` field for multi-clip change detection during playback
- Rewrote `send_decode_seek()` — collects all video clips inline (to avoid borrow checker issues), sends `SeekMulti`
- Simplified `poll_decoded_frame()` — compositing now happens in worker, receives single composited frame
- Rewrote `decode_worker()` — uses `HashMap<PathBuf, CachedDecoder>` for multi-clip caching
- Added `decode_and_composite_multi()` — decodes each clip to RGBA, composites bottom-to-top onto black canvas with transform support
- Updated `PlaybackTick` handler to compare `Vec<Uuid>` of all clip IDs for change detection

### Part 4: Preview Audio Mixing (`app.rs`)
- Added `AudioClipInfo` struct with path and time fields
- Changed `AudioDecodeRequest` to only have `SeekMulti { clips, continuous }` variant
- Added `all_audio_clips_at_position()` method to App
- Rewrote `send_audio_decode_seek()` for multi-clip collection
- Rewrote `audio_decode_worker()` — uses `HashMap<PathBuf, CachedAudioDecoder>`, decodes each clip, mixes samples additively with clamping

### Part 5: Tests Added
- **renderer_tests.rs**: `test_render_overlapping_video_two_tracks`, `test_render_overlapping_audio_two_tracks`, `test_audio_mixing_additive`
- **timeline_tests.rs**: `test_all_video_clips_at_position`, `test_video_clips_at_position_partial_overlap`
- **message_tests.rs**: `test_all_video_clips_at_position_multi_track`, `test_all_audio_clips_at_position_multi_track`, `test_composite_rgba_layers`

### Key Design Decisions
- Track ordering: video tracks stored top-to-bottom (VN...V1) in the timeline, so we iterate `.rev()` to get bottom-to-top (V1 first)
- Compositing is opaque overwrite via `blit_yuv_frame`/`blit_rgba_scaled` — upper tracks cover lower tracks, transforms reveal lower layers through offset
- Decode workers use `HashMap<PathBuf, CachedDecoder>` instead of `Option<CachedDecoder>` to cache multiple decoders simultaneously
- Borrow checker solved by collecting clip info into owned `Vec`s within scoped blocks before mutating `self`

### All Tests Passing
- `cargo test --workspace` — all 305 tests pass (up from ~258 before Brief 15)
