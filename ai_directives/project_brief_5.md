# Better timeline visuals, cut tool, hotkeys

- Introduce the concept of hotkeys
  - A is arrow which is or normal current state
  - B is the blade tool which turns the curser into a blade or sicors or something
    - A vertical line now follows the cursor whenever it is hovering over a clip
    - wherever I click it splits the clip into two
- Clips should have rounded corners and a thin dark gray border
- When dragging clips, if any overlap will trim another clip or split it or something, show that trim in real time in the UI so we can see what will happen when we lift our finger and commit the action.
- Do not let me even drag a clip to the left of 0. If I attempt to, stop the clip at 0 and let my mouse just move.

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
