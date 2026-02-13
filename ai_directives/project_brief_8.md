# Better Media Management

Right now the media management is looking rough. Let's fix that to look a bit more like Davinci Resolve's.

- Rather than a list of file names, make it a grid of buttons consisting of rounded rectangles with the file name centered under it
  - Inside the rounded rectangle should be a single frame from the video so we can kind of tell what video it is.
  - When the user hovers over a source clip, highlight it with a border
- Allow the user to DRAG media onto the timeline.
  - When we initially drag the piece of media, show an onion skin (a faded copy) of what is essentially the clip button (being the rounded rect with the frame displayed and the file name text) being dragged with our cursor. This indicates to us what we are dragging.
  - Once the cursor is dragged onto the timeline, show the video in the timeline the same way it would show if we had added it with the Add To Timeline button and dragged it around
  - Once we let go, add the clip to the timeline where it is while we are dragging it.
  - If we let go in an invalid area (for now this is just off the timeline) do not add the clip to the timeline and just remove the onion skin
  - We want to be able to use this same functionality for other buttons in the future. Develop for that in mind.
