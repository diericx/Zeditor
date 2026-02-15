# Rendering Profiling and Feedback

We are currently having some render time issues and what I would first like us to do is get some feedback.

Implement profiling that will give us insight into what areas of the render pipeline are taking the longest.

At a high level at the bottom of the screen while rendering we want to see how long the render has taken, what frame we are on out of the total frames, percentage complete, and when it is done show time it took to render.

At a lower level produce some granular metrics about how much time was spent in total on each stage of the render pipeline.

If you can, and if you think this would help, record this info per frame so we can see a line graph over time of the performance and potentially identify moments in the timeline that are causing slowdowns.

Produce this report however you see best
