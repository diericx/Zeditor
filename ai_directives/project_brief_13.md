# Effects

I want this program to be a free, open and living platform that will grow with community development. Therefore, I would like to create a system where users can create their own video and audio effects and have a marketplace of effects that can be added and used. There would be built in video effects like transform (including pos offset, scale, etc.), flip video horizontal or vertical, etc. and built in audio effects like gain (simple volume control). Uers could then create their own video effects like cartoonize or audio effects like vocal isolation or noise reduction.

If possible create a system where we can safely allow community to build effects while being able to create simple effects ourselves. Our system effects should be developed in the same way as community effects so that they can act as documentation. Make sure we can potentially allow for more complicated effects like face detection or object tracking.

If you decide to use something like WASM for effects, create a new code area where we have our built-in effects in Rust and build them at the same time we build our app and then ship them with our app.

Focus on implementing just Video effects for now.

- Create a simple tab menu where the source library is that has "Project Library" and "Effects". Project library is the default selection and is what is currently the Source Library.
- Effects tab is a simple text list of built in effects for now
- When a clip in the timeline is selected show an "effects" window to the right of the timeline that shows the controls for the effects for the current selected timeline clip
- Allow a user to add an effect to a clip in the timeline by clicking the effect and dragging onto the clip, or simply clicking a button if that is easier and there is too much scope right now
- Effects should be able to add their own functionality into the render pipeline while also accepting user input
  - For now, only expose basic user input like numbers (for x and y offset and scale for transform)
- Effects should show in the render output as well as in the editor playback window
- Implement a single simple Transform effect
  - Clips will default to how they are positioned now. Let transform offset the and y position from user input.
