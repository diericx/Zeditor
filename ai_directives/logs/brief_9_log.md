## Execution Log

### Phase 0: Serialization Safety — Round-Trip Guarantee

- Added `PartialEq` derive to `Track`, `Timeline`, `SourceLibrary`
- Implemented manual `PartialEq` for `Project` (skips `command_history` which is intentionally transient/`#[serde(skip)]`)
- Added `test_project_file_roundtrip_full_data` test that catches any future `#[serde(skip)]` regressions

### Phase 1: Core — .zpf File Format with Semver Versioning

- Added `semver` to workspace and `zeditor-core` dependencies
- Created `ProjectFile` envelope struct with `version: String` + `project: Project`
- Added `CURRENT_PROJECT_VERSION = "1.0.0"` and `MIN_PROJECT_VERSION = "1.0.0"` constants
- Updated `Project::save()` to wrap in `ProjectFile` envelope
- Updated `Project::load()` with two-pass deserialization: extract version → validate semver → deserialize full struct
- Added `VersionTooNew`, `VersionTooOld`, `InvalidProjectFile` error variants to `CoreError`
- Future migration hook: can match on `file_version.major` to transform raw JSON before final deserialization

### Phase 2: Core Tests

- Updated existing `test_project_save_load` to use `.zpf` extension
- Added `test_project_file_contains_version` — verifies version field in saved JSON
- Added `test_project_file_version_too_new` — version "99.0.0" → `VersionTooNew` error
- Added `test_project_file_version_too_old` — version "0.1.0" → `VersionTooOld` error
- Added `test_project_file_missing_version` — bare Project JSON → `InvalidProjectFile` error
- Added `test_project_file_roundtrip_full_data` — full save/load with clips, assets, multiple tracks

### Phase 3: UI State + Messages

- Added `project_path: Option<PathBuf>` field to `App` (None = unsaved, Some = saved/loaded)
- Added `App::title()` method: `"{name} - Zeditor"`
- Added `App::reset_ui_state()` helper: clears playback, decode, drag, thumbnails, zoom/scroll/tool
- Added `App::regenerate_all_thumbnails()`: spawns thumbnail tasks for all source library assets
- Added message variants: `SaveFileDialogResult`, `LoadFileDialogResult`, `NewProject`
- Updated `main.rs` to use `.title(App::title)` for dynamic window title

### Phase 4: Handler Wiring

- **SaveProject**: If `project_path.is_some()` → save directly. If None → open save dialog via `rfd::AsyncFileDialog`
- **SaveFileDialogResult(Some)**: Ensures `.zpf` extension, derives project name from filename stem, saves
- **SaveFileDialogResult(None)**: "Save cancelled" status
- **LoadProject(path)**: Loads via `Project::load()`, replaces project, resets UI, regenerates thumbnails
- **LoadFileDialogResult**: Dispatches to `LoadProject` or shows "Load cancelled"
- **NewProject**: Resets to `Project::default()`, clears path, resets UI
- **MenuAction::Save/Load/New**: Dispatched to corresponding message handlers (replaced "not yet implemented" stubs)
- Added `tempfile` and `serde_json` to zeditor-ui dev-dependencies

### Phase 5: UI Tests

- **Message tests** (15 new): save with/without path, dialog result some/none, extension enforcement, load replaces state, load sets path, load invalid file error, load version too new, new project resets, load clears playback, dialog result forwarding, title reflects name, save-then-save-again no dialog
- **Simulator tests** (3 new): file menu shows load project, window title reflects name, title updates after name change
- Updated `test_menu_unimplemented_actions` → split into `test_menu_new_project_dispatches` and `test_menu_save_dispatches`

### Test Summary

- Total workspace tests: **204** (was ~188 before)
- All passing on `cargo test --workspace`
- New tests cover: versioned file format, version validation errors, round-trip data integrity, save/load/new UI workflows, title updates
