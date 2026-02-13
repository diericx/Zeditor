# Saving and Loading projects

We now want the ability to save and load projects.

- Create a file type that will hold our project info with the suffix .zpf
- File type should have a version number and there should be some way for us to mark instances where newer versions of the app are incompatible with older save files
  - Create some infrastructure for converting old save files but do not implement yet. For now if it happens we will simply make them incompatible with new versions of the app and show an error message.
- By default open to an empty project called Untitled
  - NAme of project should be represented as the window title
- When you save a project from the menu, if the current project hasn't been saved open a file dialogue asking us where we want to put it
- Now that the project is named something else, change the window title
- When we save
  - the entire organization of the timeline should be captured
  - the list of clips loaded in to our source library should be saved
- Add the ability to click the load proj button and find a .zpf file
  - Load all this info back up when we load a project, and continue as normal editing the file we selected
  - With a file selected/loaded save no longer prompts a menu we can just save directly to the file
