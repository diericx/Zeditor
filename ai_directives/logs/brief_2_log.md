## Implementation Log

**Completed**: All 5 phases implemented. 56 tests pass headlessly via `cargo test --workspace`.

### What was built

**4 crates** in a Cargo workspace:

- **zeditor-core** (pure Rust): Project, Timeline, Track, Clip, MediaAsset, SourceLibrary, TimeRange, TimelinePosition, CommandHistory. Operations: add_clip, cut_at, move_clip, resize_clip, snap_to_adjacent, undo/redo, save/load (serde_json). **22 unit tests**.
- **zeditor-media** (rsmpeg 0.18 + system FFmpeg 8): FfmpegDecoder (VideoDecoder trait), probe(), thumbnail generation, FfmpegExporter (CLI subprocess). **7 integration tests** using generated test fixtures.
- **zeditor-ui** (iced 0.14): App with Elm-architecture update/view, Message enum for all editor operations, views for source library/timeline/playback. **24 tests**: 11 message-level, 10 iced_test Simulator (headless click/find/typewrite), 3 cross-crate integration.
- **zeditor-test-harness**: ffmpeg lavfi fixture generation, builder pattern for test data (MediaAssetBuilder, ClipBuilder, ProjectBuilder), assertion helpers. **2 tests**.

### Test architecture (5 layers)

1. **Unit tests** (zeditor-core): Pure logic, no deps, instant
2. **Media integration** (zeditor-media): Real FFmpeg decode/seek/probe on generated .mp4 fixtures
3. **Message-level** (zeditor-ui): `app.update(Message::...)` → assert model state, no rendering
4. **Simulator tests** (iced_test): Headless `simulator(app.view())` → `click("Play")` / `find("Timeline")` → assert messages
5. **.ice E2E** (tests/e2e/): Declarative test files ready for `iced_test::run()` when Program trait is wired

### Key decisions

- **iced 0.14** for UI: Elm architecture makes all state transitions testable without rendering; iced_test 0.14 provides headless Simulator/Emulator
- **rsmpeg 0.18** for in-process decoding (links system FFmpeg 8 via `link_system_ffmpeg`); CLI ffmpeg for export (crash isolation)
- **Snapshot undo/redo**: CommandHistory captures full Timeline snapshots, simple and reliable
