# Refining the current timeline features

The timeline is now playing back video well and clips can be moved around. Lets refine it a bit.

- Scrolling alone in the timeline should move left and right
- Scrolling while holding left-alt should zoom in and out
- Do not let me drag a clip to the left of the 0 time mark
- BUG: Clicking play (space bar) while the playback cursor is BEFORE the first clip (say at second 5 but the first clip has been placed at second 30) shows the last frame displayed rather than what should or would be displayed here (which is black frames)
  - If the timeline is played when there is no content, show black as that is what would be rendered.
- BUG: when the playback cursor reaches the end of a clip and there is no clip after it it playback stops
  - IT should continue playback but show black
- BUG: When the playback cursor reaches the end of a clip and there IS a clip after it, it simply stop on the last frame of the ended clip
  - It should transition into playing the next clip
