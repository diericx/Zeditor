## Execution Log

### Phase 1: Core Data Model (`zeditor-core`)
- Added `TrackType` enum (Video/Audio) with `#[serde(default)]` to `timeline.rs`
- Added `group_id: Option<Uuid>` to `Track` and `link_id: Option<Uuid>` to `Clip`
- Updated `Track::new()` to accept `TrackType`; added `Track::video()` / `Track::audio()` constructors
- Updated `Timeline::add_track()` to accept `TrackType`
- Added `add_track_with_group()`, `group_members()`, `find_linked_clips()`, `find_paired_audio_track()`
- Added `add_clip_with_audio()` — creates paired video+audio clips with shared `link_id`
- Added `move_clip_grouped()`, `resize_clip_grouped()`, `cut_at_grouped()` — all apply same delta to linked clips
- Updated `Project::new()` to create grouped Video 1 + Audio 1 tracks with shared `group_id`
- Fixed all existing test call sites (~30 places) from `add_track("name")` to `add_track("name", TrackType::Video)`
- Added `link_id` field to all manually-constructed Clip literals (`add_clip_trimming_overlaps` right_piece, `cut_at` left/right)
- Added 10 new core tests (track type, grouping, linked clips, grouped ops, serialization roundtrip, undo/redo)
- All 40 core tests pass

### Phase 2: Audio Decoder (`zeditor-media`)
- Created `audio_decoder.rs` with `FfmpegAudioDecoder` and `AudioFrame` structs
- Uses `SwrContext` to convert to f32 interleaved PCM at native sample rate
- Methods: `open()`, `decode_next_audio_frame()`, `seek_to()`, `sample_rate()`, `channels()`
- Added `NoAudioStream` error variant to `error.rs`
- Created 6 audio decoder tests (open, decode, multi-frame, seek, no-audio error, valid f32 range)
- All 6 tests pass

### Phase 3: Grouped UI Interactions (`zeditor-ui`)
- Audio tracks get blue-tinted background color in canvas rendering
- Updated `AddClipToTimeline` and `PlaceSelectedClip` to create paired audio clips when `has_audio=true`
- Updated `MoveClip`, `ResizeClip`, `CutClip` handlers to use grouped variants when clip has `link_id`
- Split `clip_at_position()` into video-only; added `audio_clip_at_position()` for audio tracks
- Added 8 new UI tests (grouped add/cut/move/resize, audio-only placement, undo/redo, clip_at_position filtering)
- All 137 workspace tests pass

### Phase 4: Audio Playback (`zeditor-ui`)
- Created `audio_player.rs` — rodio wrapper (OutputStream, Sink) with `queue_audio()`, `stop()`, `pause()`, `play()`
- Added `AudioDecodeRequest` enum and `DecodedAudio` struct to `app.rs`
- Added audio fields to App: `audio_player`, `audio_decode_tx/rx`, `audio_decode_clip_id`, `audio_decode_time_offset`
- Updated `boot()` to spawn audio decode worker thread alongside video decode thread
- Updated `PlaybackTick` to check audio clip transitions and call `poll_decoded_audio()`
- Updated `Play`/`Pause` to control audio player and audio decode thread
- Added `send_audio_decode_seek()`, `send_audio_decode_stop()`, `poll_decoded_audio()` methods
- Added `audio_decode_worker()` function mirroring video decode worker pattern
- Added `TestAudioSender` and `new_with_test_channels()` to test helpers
- Added 5 audio playback tests (clip ID set, gap detection, clip boundary transition, frame polling, pause)
- All 142 workspace tests pass

### Phase 5: Polish & Edge Cases
- Verified undo/redo works for all grouped operations (existing tests)
- Made `move_clip_grouped` and `resize_clip_grouped` resilient to independently-deleted linked clips (skip missing instead of error)
- Asset without audio handled gracefully (existing tests confirm video-only path)
- Serialization roundtrip tested (existing test)
- Fixed unused `self` import warning in timeline_tests.rs
- Final verification: all 142 tests pass
