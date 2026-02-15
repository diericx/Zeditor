# Refining project rendering

Rendering is working okay right now but it needs to be refined. In reality we should have two settings:

1. Project Settings
   1. A project should have resolution and framerate defined. This defines what the playback window looks like (e.g. if it is 1000x1000 it would show a black square, 1920x1080 would show a rectangle)
   2. We can then edit our video within this canvas scaling, rotating, and stretching video if we want. If we set a video to be smaller within this canvas it will show black on the canvas that it does not cover
2. Render settings
   1. When we set our render settings it will take our clip within our project canvas (1920x1080 for example) and render it out.
      1. So say we put a 500x500 clip in the center of the canvas which is set to 1920x1080 in our project settings. It would have black all around the edges. If we then rendered at 1920x1080 it should have just that, black around the edges as the small video is centered within the larger canvas.
      2. Say we then render it at 1280x720 which is still 16:9. It should then scale the output down but maintain the fidelity of our designed image, being that small clip within a 16:9 canvas that was originally 1920x1080.
      3. Say we render it at another resolution, it should scale the entire canvas that we designed rather than just the clip

Eventually we want to enable effects that will allow us to scale and rotate clips within our project canvas to be rendered how they appear during editing but for now, if a clip does not fit within our project resolution default to scaling it such that it fits within our canvas and is dead center. It is already doing that perfectly when playing back during editing. That needs to be reflected in rendering.
