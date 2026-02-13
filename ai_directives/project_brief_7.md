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
