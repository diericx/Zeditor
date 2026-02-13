# Menu

Add a traditional menu with some buttons but little functionality yet. We want to eventually have a robust menu system with drop downs and buttons in drop downs that then open sub menues, etc.

- At the top of the screen there should be a main menu bar (simple flat gray with a darker gray underline and white text) with the following buttons: "File"
  - Buttons are rounded rectangles within the menu bar
  - Hovering over a button highlights it in a lighter gray
  - Clicking a button opens its submenu
  - Once a menu is clicked, and it's submenu is open, the user can then hover the mouse across the main menu onto other main menu buttons where it will then close the old menu and open the one the mouse is now hovering over
  - A submenu stays open until a menu item is clicked, the main menu button for the open menu is clicked (essentially toggling it off), or another part of the screen is clicked. Essentially once a menu is opened, the rest o the screen becomes just a "click off zone", meaning clicks on other ui elements should not register other than to close the menu.
- When you click File it should open a new submenu
  - Submenus are rounded corner rectangular list of new menu items shown with the left edge aligned to the left of the menu button
  - Show three buttons in the drop down: "New Project", "Load Project", "Save" and "Exit"
  - Clicking Exit closes the program, the rest are unimplemented yet
- When you click Edit it should open a new submenu
  - Available actions are "Undo" and "Redo"
  - Connect those buttons to the existing actions for undo and redo.

---

## Execution Log

### Implementation (Brief 7 - Menu Bar System)

**Files modified:**
1. `crates/zeditor-ui/src/message.rs` — Added `MenuId` (File, Edit), `MenuAction` (NewProject, LoadProject, Save, Exit, Undo, Redo) enums. Added `Message` variants: `MenuButtonClicked`, `MenuButtonHovered`, `CloseMenu`, `MenuAction`, `Exit`.
2. `crates/zeditor-ui/src/app.rs` — Added `open_menu: Option<MenuId>` state field. Added update handlers for all menu messages (toggle open, hover-to-switch, close, dispatch actions). Modified `KeyboardEvent` handler to swallow keys when menu is open (Escape closes menu). Added view methods: `view_menu_bar()`, `menu_bar_button()`, `view_dropdown()`, `menu_item()`. Modified `view()` to include menu bar at top and `stack!`-based overlay with click-off zone when menu is open.
3. `crates/zeditor-ui/tests/message_tests.rs` — Added 12 new tests covering: open/toggle/switch menus, hover-to-switch, close, action dispatch (undo/redo/unimplemented stubs), escape key, keyboard swallowing.
4. `crates/zeditor-ui/tests/simulator_tests.rs` — Added 5 new tests: menu bar visible (File/Edit buttons), click File opens submenu items, click Edit opens submenu items, render with open File menu, render with open Edit menu.

**Design decisions:**
- Used iced `stack!` widget for layering: base content → click-off `mouse_area` → `opaque(dropdown)`. This prevents clicks from reaching underlying UI while menu is open.
- Menu buttons use `mouse_area` with `on_enter` for hover-to-switch behavior (only activates when a menu is already open).
- Dark theme styling: menu bar `rgb(0.20, 0.20, 0.22)`, dropdown `rgb(0.22, 0.22, 0.24)`, hover highlights `rgb(0.32, 0.32, 0.35)`, white text.
- `MenuAction::Exit` delegates to `Message::Exit` which calls `iced::exit()`.
- `MenuAction::Undo`/`Redo` delegate to existing `Message::Undo`/`Message::Redo` handlers.
- Unimplemented actions (NewProject, LoadProject, Save) set status message "not yet implemented".

**Test results:** All 174 tests pass (0 failures).
