## Generative 3D Music Visualizer (Rust + WebGPU + WebAudio)

### Run (Web)

- Build: `npm run build:web`
- Serve: `node server.js` (serves `crates/app-web` with correct headers)
- Open: visit `http://localhost:8080`
- Dev shortcut: `npm run dev:web` then `npm run open:web`

Controls in browser:

- Click canvas to start audio
- Drag a circle to move a voice in XZ plane (updates spatialization)
- Click a voice: mute; Shift+Click: reseed; Alt+Click: solo
- Keys: H (toggle help), R (reseed all), Space (pause/resume), + / - (tempo)

Headless test:

- `npm run ci:web` builds, serves, and runs a Puppeteer test locally

### Run (Native)

- Build: `npm run build:native`
- Run: `npm run native`

### Workspace crates

- `app-core`: shared music generation and state
- `app-native`: native window + WebGPU rendering + basic audio
- `app-web`: web WASM front-end with WebGPU + WebAudio
