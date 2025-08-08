## Generative 3D Music Visualizer (Rust + WebGPU + WebAudio)

- Native app: `cargo run -p app-native`
- Web (WASM): uses `wasm-pack` to build and a static `index.html` to run (to be added next)

Workspace crates:
- `app-core`: shared music generation and state
- `app-native`: native window + WebGPU rendering (skeleton)
- `app-web`: web WASM front-end (TBD)

