## Generative 3D Music Visualizer (Rust + WebGPU + WebAudio)

[![Web build and headless test](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml/badge.svg)](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml)

### Project status

- Web front-end (WASM) is running with:
  - 3 voices, spatial audio (Web Audio + PannerNode)
  - Lush ambient effects: global Convolver reverb and dark feedback Delay bus with per-voice sends and a master bus
  - Mouse-driven FX: corner-based saturation (clean ↔ fizz) and opposite-corner delay emphasis; visuals have inertial swirl motion and click ripples
  - Start overlay to initialize audio (Click Start; canvas-click fallback)
  - Drag voices in XZ plane; click to mute, Shift+Click reseed, Alt+Click solo
  - Keyboard: R (reseed all), Space (pause), + / - (tempo), M (master mute)
  - Starts muted by default; press M to unmute the master bus
  - Dynamic hint shows current BPM, paused, and muted state
  - Rich visuals: instanced voice markers with emissive pulses, ambient waves background, post bloom/tonemap/vignette; optional analyser-driven spectrum dots
- Native front-end renders and plays synthesized audio; parity improving:
  - Equal-power stereo panning from X position, multiple waveforms (sine/square/saw/triangle)
  - Gentle master saturation (arctan curve); hover highlight parity; renderer uses `scene.wgsl`

### Demo

- Local: see Run (Web) below. After `npm run dev:web`, open `http://localhost:8080`.
- Hosted: deploy with Cloudflare Workers (see Deploy). A public demo link can be added here when available.

### Requirements

- Node 20+
- Rust (stable, 2021 edition)
- wasm-pack (install: `curl -sSfL https://rustwasm.github.io/wasm-pack/installer/init.sh | sh`)
- Desktop browser with WebGPU enabled

Notes:

- WebGL fallback is intentionally avoided; WebGPU is required.
- If audio does not start, click the Start overlay and press M to unmute the master bus.

### Run (Web)

- Build: `npm run build:web`
- Serve: `node server.js` (serves `crates/app-web` with correct headers)
- Open: visit `http://localhost:8080`
- Dev shortcut: `npm run dev:web` then `npm run open:web`

Quick controls (browser):

- R: reseed all • Space: pause/resume • +/-: tempo • M: master mute
- Click a voice to mute; Alt+Click to solo; Shift+Click to reseed a voice; drag to move in XZ

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
- Keys: R (reseed all), Space (pause/resume), + / - (tempo), M (master mute)
- Mouse position maps to master saturation and delay; moving the pointer leaves a “water-like” trailing swirl in visuals

Headless test:

- `npm run ci:web` builds, serves, and runs a Puppeteer test locally

### Continuous Integration

- GitHub Actions workflow runs on push/PR to main:
  - Builds the web bundle and executes the headless browser test
  - On push to `main`, deploys to Cloudflare Workers via Wrangler
  - Requires repo secrets: `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
  - Workflow file: `.github/workflows/web-ci.yml`
  - CI tolerates missing WebGPU in headless by skipping engine-coupled assertions

### Run (Native)

- Build: `npm run build:native`
- Run: `npm run native`
  - Optional: `npm run native:smoke` runs a short smoke test and exits

### Media

- Screenshots/GIFs live in `docs/media/`.
- Place files like:
  - `docs/media/screenshot-1.png` – main scene
  - `docs/media/screenshot-2.png` – hint overlay
  - `docs/media/loop-1.gif` – waves background with ripples and pulses

Links:

- [Screenshot 1](docs/media/screenshot-1.png)
- [Screenshot 2](docs/media/screenshot-2.png)
- [Loop GIF](docs/media/loop-1.gif)

### Workspace crates

- `app-core`: shared music generation and state
- `app-native`: native window + WebGPU rendering + basic audio
- `app-web`: web WASM front-end with WebGPU + WebAudio

### Docs

- Project Spec: `docs/SPEC.md`
- Project TODO: `docs/TODO.md`
