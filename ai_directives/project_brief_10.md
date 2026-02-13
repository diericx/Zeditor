# Rendering

We now want to implement the final core piece of our app; video rendering.

- Create a new menu item File -> Render
- By default just render the timeline in h264 in an mkv container with the superfast preset using CRF 22.
- Places with no clip should render black
- Places with a clip should render that clips video and audio
- Render until the last clip then end. In the future we will want to implement adjustable start and end render timecodes but for now default to the end of the last clip in the timeline
