## Generative 3D Music Visualizer (Rust + WebGPU + WebAudio)

[![Web build and headless test](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml/badge.svg)](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml)

### Project status

- Web front-end (WASM) is running with:
  - 3 voices, spatial audio (Web Audio + PannerNode)
  - Lush ambient effects: global Convolver reverb and dark feedback Delay bus with per-voice sends and a master bus
  - Start overlay to initialize audio (Click Start; canvas-click fallback)
  - Drag voices in XZ plane; click to mute, Shift+Click reseed, Alt+Click solo
  - Keyboard: R (reseed all), Space (pause), + / - (tempo), M (master mute), O (orbit on/off)
  - Starts muted by default; press M to unmute the master bus
  - Dynamic hint shows current BPM, paused, and muted state
  - Rich visuals: instanced voice markers with emissive pulses, animated orbiting ring particles, subtle vignette, optional analyser-driven spectrum dots
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

### Pre-commit Check

- Run all checks and tests locally: `npm run check`
  - Rust: `cargo fmt --check`, `cargo clippy` (deny warnings), `cargo test` (workspace), `cargo build -p app-native`
  - Web: build, serve, and execute the headless browser test

### Deploy (Cloudflare Workers)

This repo is configured to deploy via Cloudflare Workers; headers (COOP/COEP/CORP) are set in `worker.js`.

- Build: `npm run build:web`
- Deploy: `npx --yes wrangler deploy`
  - Config: `wrangler.toml` (assets at `crates/app-web`)
  - Worker adds the required headers and sensible cache-control for `.wasm`/`.js`/HTML

Controls in browser:

- Click Start to initialize audio (canvas click also works)
- Drag a circle to move a voice in XZ plane (updates spatialization)
- Click a voice: mute; Shift+Click: reseed; Alt+Click: solo
- Keys: R (reseed all), Space (pause/resume), + / - (tempo), M (master mute), O (orbit on/off)

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
