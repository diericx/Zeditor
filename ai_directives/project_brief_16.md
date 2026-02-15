# Real per pixel effects

Convert the current effect pipeline from a pure parametric design to a full per pixel design. Each rust effect should still expose parameters that it can use, but it should have a core compute function that accepts pixels and outputs pixels.

Do not optimize for now and do not implement WASM.
