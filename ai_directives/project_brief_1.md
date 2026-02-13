# Zeditor

## Development

The name of this project is Zeditor.

This project is aimed to be developed by coding agents mostly, and therefore whenever doing anything with this project these briefs should be read for context.

Maintain these principles at all times:

- Test Driven Development
  - Any new feature developed should have tests such that an AI agent can validate the ENTIRETY of a new feature. If you cannot write a test for the feature end to end to confirm it is working, stop and focus on that.
- Fault Tolerance
  - We are developing this app to solve for crappy, crashy video editors. Make sure this app won't crash!
- Cross Platform
  - This app should be developed with cross platform editing in mind, but for now focus on getting it running on Arch Linux with ffmpeg8 installed.
- Code scalability
  - The goals for this project are long term and ambitious. Develop in such a way that we can iterate on features and get initial versions working with smaller scopes, but will be able to scale up the scope progressively over time.
- Codified changes
  - When executing a specific brief, keep a log at the bottom of the file of what was done so we can come back to it if we need to. We are doing this so we don't have to constantly keep every decision in our context/memory. We can write it down and reference when necessary. Do not alter what was originally in thee files, append your logs to the bottom.

## The project

We are aiming to create a simple video editor that can become as feature complete as KDEnlive. We eventually want to be able to do things like the following. Note this is not an exhaustive list, but some complex features we plan on adding so you can get an idea of what type of feature scale to build for.

### Manage a source library

- Load in a large amount of clips of videos of all types of codecs
- Handle awkward codecs or quirks such as codecs we have difficulty scrubbing through (super high res h265 for example, or some high res codec we don't have hardware decoding for)
  - Support generating lower res proxy files for each clip in Prores or HDxNR
- Be able to see a library of our loaded clips with thumbnails and file names
- Search files by name
- Sort our files by name, date, etc.
- Create tags and assign them to our files for large project organization
- Proxy files are identified in some way by just pointing to a folder and connecting them via file name after generated
- Timelines can later be saved by referencing the file path, if the files are lost one can point at a new file path to search for the same file names to reconnect the timeline

### Video playback

- A window shows video playback
- The playback window shows the frame of video where the playback head is at in the timeline, with small play and pause button below the video itself

### Create new timelines

- A project will have a timeline where we can drag in source clips and manipulate them by:
  - cutting them into to distinct clips at a certain time with a cut tool (so a source clip can exist in the timeline unlimited times as it is dragged in, cut, etc.)
  - hovering to the end of a clip should change the cursor and allow us to manipulate the size of the clip to make it smaller, or larger up until the max size of said source clip
  - clicking the center of a clip and dragging them around the timeline to organize
  - timeline clips snap to the end of other clips letting us easily create cohesive clip narratives
- A timeline has a small bar at the top indicating the time
- There is a cursor head indicating where in the timeline our playback is
- Timelines and projects can be saved and loaded at a later date

### Rendering timelines (exporting a project)

- We can export our timeline to a single clip in codecs such as h264, h265, Av1 etc using ffmpeg.
