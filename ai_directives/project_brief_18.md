# Small bug fixes

- If I begin playback in the middle of a single clip it is fine. If I begin playback in the middle of overlapped clips audio is super choppy.
- Transform offset should be a number field not a slider
- Playback in the editor crashes if you move a clip too far with offset which is easy to do by accident
  - thread '<unnamed>' (2978988) panicked at crates/zeditor-core/src/pipeline.rs:159:24:
    range end index 17179868872 out of range for slice of length 2073600
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrack
