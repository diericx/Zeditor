# Add Audio Support

Remember to write unit tests and end to end tests for as much of this as possible.

- Add concept of grouped tracks
  - Tracks can be grouped vertically. So if there are 3 video tracks and 3 audio tracks, 6 tracks can all be grouped
  - Grouped tracks all move at the same time when dragged
  - If you drag the end of a track that is grouped it will reduce its size like normal, all tracks in the group are reduced by the same amount even if they are offset in the timeline
    - All grouped tracks should show the clip being trimmed in real time while dragging the end not just the track you clicked on
- Add audio tracks
  - When adding a new clip, the audio should also be added on the cooresponding audio track
  - Projects all start with 1 video track and 1 audio track
  - When dragging grouped clips around, they should function the same as how we implemented earlier where it shows how the clips would be inserted even if they haven't been inserted yet (show clips being trimmed, show oother clips being cut so it can fit inside, etc) for all clips including audio clips. While dragging you should see exactly what will happen if you let go and commit the action.
- Add audio playback support
  - If the cursor continues off the end of a clip into an area with no audio clips it should stop all audio playback
  - if the cursor transitions off of one clip onto another it should stop playing that audio and play the audio of the next clip, correctly playing at the point where the clip starts and ends according to how it was trimmed
