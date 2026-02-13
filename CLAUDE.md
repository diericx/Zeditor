# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Zeditor is a video editor built in Rust, aiming to become feature-complete like KDEnlive. It uses iced 0.14 for the GUI and rsmpeg/FFmpeg 8 for media handling. The project is developed primarily by AI agents — read `ai_directives/` briefs for full context on completed work and design decisions.

## Agent Directives

- Read all project briefs before each task
- Test Driven Development
  - Any new feature developed should have tests such that an AI agent can validate the ENTIRETY of a new feature. If you cannot write a test for the feature end to end to confirm it is working, stop and focus on that.
- Fault Tolerance
  - We are developing this app to solve for crappy, crashy video editors. Make sure this app won't crash!
- Cross Platform
  - This app should be developed with cross platform editing in mind, but for now focus on getting it running on Arch Linux with ffmpeg8 installed.
- Code scalability
  - The goals for this project are long term and ambitious. Develop in such a way that we can iterate on features and get initial versions working with smaller scopes, but will be able to scale up the scope progressively over time.
- Codified changes
  - When executing a specific brief, keep a log at the bottom of the file of what was done so we can come back to it if we need to. We are doing this so we don't have to constantly keep every decision in our context/memory. We can write it down and reference when necessary. Do not alter what was originally in thee files, append your logs to the bottom.

## Build & Test Commands

```bash
cargo test --workspace              # Run all tests (~137 tests)
cargo test -p zeditor-core          # Core domain logic tests only
cargo test -p zeditor-media         # FFmpeg integration tests only
cargo test -p zeditor-ui            # UI tests (message-level, simulator, canvas)
cargo test -p zeditor-test-harness  # Test fixture generation tests
cargo test -p zeditor-ui -- test_name  # Run a single test by name
cargo run -p zeditor-ui --bin zeditor  # Run the application
```

Media tests require FFmpeg 8 installed on the system.

## Architecture

Four-crate workspace with strict dependency layering:

```
zeditor-ui (binary, iced GUI)
  ├── zeditor-core (pure domain logic, no external deps beyond serde/uuid)
  └── zeditor-media (FFmpeg integration via rsmpeg)
        └── zeditor-core

zeditor-test-harness (dev-only: builders, fixtures, assertions)
```

### zeditor-core

Pure Rust domain types and operations. No UI or media dependencies.

- `Timeline` / `Track` / `Clip` — timeline model with overlap detection, cut, move, resize, snap
- `TimelinePosition(Duration)` — newtype for timeline positions
- `TimeRange { start, end }` — inclusive-exclusive time ranges
- `MediaAsset` / `SourceLibrary` — asset metadata and collection
- `CommandHistory` — undo/redo via full `Timeline` snapshots
- Key operations: `add_clip_trimming_overlaps`, `cut_at`, `move_clip`, `snap_to_adjacent`, `preview_trim_overlaps`, `preview_snap_position`

### zeditor-media

FFmpeg integration using rsmpeg 0.18 with system FFmpeg 8.

- `VideoDecoder` trait (mockable) with `FfmpegDecoder` implementation
- `VideoFrame { width, height, data: Vec<u8>, pts_secs }` — RGB24 pixel data
- `probe(path)` — extract media metadata into `MediaAsset`
- Background decode uses multithreaded FFmpeg (`thread_count = 0`)

### zeditor-ui

iced 0.14 Elm-architecture GUI. Entry point: `iced::application(App::boot, App::update, App::view)`.

- **Message enum** drives all state transitions (see `src/message.rs`)
- **App** struct holds `Project`, playback state, decode channel, zoom/scroll
- **TimelineCanvas** — iced `canvas::Program` for custom timeline rendering with drag, resize, blade tool, snap preview, trim preview overlays
- **Decode thread** — background worker receives `DecodeRequest`, returns `DecodedFrame` via `mpsc` channels; frames scaled to max 960x540
- **Playback timing** — wall-clock based (`Instant`), independent of tick rate
- **Tool modes**: Arrow (A key) for select/drag, Blade (B key) for cut
- **Layout**: left panel (source library), right panel (video viewport), bottom (timeline canvas)

### zeditor-test-harness

- `MediaAssetBuilder` / `ClipBuilder` / `ProjectBuilder` — fluent test builders
- `generate_test_video(name, duration, width, height)` — creates .mp4 via `ffmpeg -f lavfi -i testsrc=...`

## Test Layers

1. **Unit** (zeditor-core): Pure logic, instant, no deps
2. **Media integration** (zeditor-media): Real FFmpeg on generated fixtures
3. **Message-level** (zeditor-ui): `app.update(Message::...)` → assert state changes
4. **Simulator** (zeditor-ui): Headless iced via `iced_test::simulator()` → click/find widgets
5. **Cross-crate** (zeditor-ui): Full import → decode pipeline

## Key API Gotchas

- **iced 0.14 application**: First arg is `boot` fn (not title). Use `.title("...")` method.
- **iced 0.14 canvas**: `update()` returns `Option<canvas::Action<Message>>`. Use `Action::publish()`, `Action::capture()`, `Action::request_redraw()`.
- **Element lifetime**: `Element<'_, Message>` — Rust 2024 edition requires explicit lifetime annotation.
- **iced_test Simulator**: `find()` and `click()` take `&mut self`. Simulator borrows the Element — must drop simulator before mutating app state.
- **rsmpeg FFI constants**: Use `rsmpeg::ffi::AVMEDIA_TYPE_VIDEO` (not `AVMediaType_AVMEDIA_TYPE_VIDEO`).
- **rsmpeg seek**: `AVSEEK_FLAG_BACKWARD` is `u32`, must cast to `i32`.
- **SwsContext::get_context()**: Takes 10 positional args plus 3 `Option` params.
- **time::every()**: Requires `"tokio"` feature (default `thread-pool` doesn't support it).

## Development Principles (from ai_directives/project_brief_1.md)

- **Test-driven**: Every feature must have tests an AI agent can run to validate end-to-end.
- **Fault tolerance**: The app must not crash — we're solving for unreliable video editors.
- **Cross-platform**: Target Arch Linux + FFmpeg 8 now, keep cross-platform in mind.
- **Code scalability**: Build for iterative scope expansion.
- **Codified changes**: Append execution logs to the bottom of brief files when executing them.

## Rust Edition & Toolchain

- Rust 2024 edition, Rust 1.93
- Workspace resolver = "2"
