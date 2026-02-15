## Implementation Log

### Completed

1. **ToolMode enum** — Added `ToolMode` (Arrow/Blade) to `message.rs` with Default derive
2. **App tool_mode field** — Added `tool_mode` to `App` struct, A/B keyboard handling, passed to canvas
3. **Blade mode behavior** — Click on clip body in Blade mode emits `CutClip`; cursor tracking in state; orange vertical line drawn over clips; crosshair cursor in blade mode
4. **Rounded corners + border** — Replaced `fill_rectangle` with `rounded_rectangle` Path + fill + stroke (4px radius, dark gray 1px border)
5. **Drag clamp at time 0** — `CursorMoved` handler clamps `current_x >= offset_px - scroll_offset` so clip left edge can't go negative
6. **TrimPreview + preview_trim_overlaps** — Read-only preview method on `Track` returns what each overlapping clip would become
7. **Live drag preview** — During drag, red semi-transparent overlay (a=0.25) drawn over portions that would be trimmed/removed
8. **Tests** — 10 new tests: 5 for preview_trim_overlaps, 3 for tool mode hotkeys, 2 for canvas (blade click, drag clamp)
9. All 91 tests pass
