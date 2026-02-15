# Rendering issues

We are currently having some render time issues. Here are the symptoms. Note that I am working with 4k source video.

- A timeline with a single 10 second long clip takes 7 seconds to render, that is great
- If I add a single effect to the clip render time is not affected
- If I add a single clip on another track (with or without overlaying onto the original clip) the render goes all the way up to 50 seconds
- I I overlay the second clip onto the first clip and add a single effect to both tracks render time is around 1 minute.

Figure out why render time jumps so signficantly when adding another track like this.
