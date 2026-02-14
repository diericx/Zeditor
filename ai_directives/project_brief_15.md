# Functional track layering

- When two audio clips overlap, play them both at the same time
- When video clips overlap, render them both but layer them correctly
  - Layer the same way they are visually on the timeline; V1 is on the bottom and VN is on the top
  - If there are two video tracks
    - If V2 fully covers the screen it would be the only thing showing while playing
    - If we then transform V2 to the right for example we would then see part of V1 and part of V2
