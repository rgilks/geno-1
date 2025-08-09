## Generative 3D Music Visualizer (Rust + WebGPU + WebAudio)

[![Web build and headless test](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml/badge.svg)](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml)

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

### Host statically (GitHub Pages or any static host)

Artifacts are in `crates/app-web/pkg` after `npm run build:web`.

- GitHub Pages (manual):
  - Build: `npm run build:web`
  - Copy `crates/app-web/index.html` and the entire `crates/app-web/pkg/` directory to your Pages branch/site root
  - Ensure COOP/COEP headers if you need SharedArrayBuffer; otherwise, WebGPU does not require them
  - Open your Pages URL

- Any static host:
  - Serve the folder `crates/app-web/` with `index.html` and `pkg/` available at the same path
  - If you need COOP/COEP (e.g., for future features using SAB), configure headers
  - Sample dev server with headers is `server.js`

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
  - On push to `main`, deploys to Cloudflare Workers via Wrangler
  - Requires repo secrets: `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
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
