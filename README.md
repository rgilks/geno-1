## Generative 3D Music Visualizer (Rust + WebGPU + WebAudio)

### Project status

- Web front-end (WASM) is running with:
  - 3 voices, spatial audio (Web Audio + PannerNode)
  - Drag voices in XZ plane; click to mute, Shift+Click reseed, Alt+Click solo
  - Keyboard: H (help), R (reseed all), Space (pause), + / - (tempo)
  - Dynamic hint shows current BPM and pause state
- Native front-end renders and plays basic synthesized audio (parity improving)

### Requirements

- Node 20+
- Rust (stable, 2021 edition)
- wasm-pack (install: `curl -sSfL https://rustwasm.github.io/wasm-pack/installer/init.sh | sh`)
- Desktop browser with WebGPU enabled

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

### Continuous Integration

- GitHub Actions workflow runs on push/PR to main:
  - Builds the web bundle and executes the headless browser test
  - Workflow file: `.github/workflows/web-ci.yml`

### Run (Native)

- Build: `npm run build:native`
- Run: `npm run native`

### Workspace crates

- `app-core`: shared music generation and state
- `app-native`: native window + WebGPU rendering + basic audio
- `app-web`: web WASM front-end with WebGPU + WebAudio

### Docs
- Project Spec: `docs/SPEC.md`
- Project TODO: `docs/TODO.md`
