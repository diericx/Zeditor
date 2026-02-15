# Multiple tracks

Don't worry about implementing the rendering of all these stacked tracks, focus on the timeline editing aspect.

- Make the track view fill the rest of the vertical space, all the way until down until the system messages bar
- Add a header to the left of each track that has the static track name (V1-VN for video, A1-AN for audio) that stays put while scrolling so you know which track is which
- If you right click on an audio track show a pop up menu with two options; "Add Audio Track Above" and "Add Audio Track Below"
- If you right click on a video track show a pop up menu with two optiosn; "Add Video Track Above" and "Add Video Track Below"
- Video tracks go on top and audio tracks are stacked vertically on top of audio tracks and there should be no way to interweave them
- You should be able to drag solo video clips (not grouped to any audio) onto any video track and vice versa for audio tracks
- Tracks should behave as they do in KDENLive. starting from the center, tracks are kind of linked such that if you try to drag grouped tracks around they will rise out from the center two vide and audio tracks. from the center there is V1 on top and A1 on bottom. If you have 2 vide and 2 audio tracks it would go

V2
V1
A1
A2

If you try to drag a video onto V2 that is grouped to audio it should also move the audio to A2 like this

V2 ==V===
V1
A1
A2 ==A===

If you say have 3 video tracks and only two audio tracks you would not be able to drag the grouped clip to V3 because there is no cooresponding audio track.
