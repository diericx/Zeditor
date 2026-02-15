# Real per pixel effects

Convert the current effect pipeline from a pure parametric design to a full per pixel design. Each effect should still expose parameters that can be set in the UI and used in the computational function of the effect, but it should have a core compute function that accepts pixels and outputs pixels.

Do not optimize too much for now and do not implement WASM.
