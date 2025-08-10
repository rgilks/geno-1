## Pre-commit safety checks

To catch build breakages early, this repo includes a Husky pre-commit hook that runs the full `npm run check`:

```
npm run check
```

That script enforces Rust fmt/clippy, runs core unit tests, builds the web bundle, serves it, and executes the headless Puppeteer test. If any step fails, the commit is aborted.

If you prefer Git native hooks, enable the lightweight `.githooks/pre-commit` by pointing Git to that directory:

```
git config core.hooksPath .githooks
```

The `.githooks` variant runs `npm run check:rust` (fast Rust-only checks) and can be combined with Husky as needed.

## Generative 3D Music Visualizer (Rust + WebGPU + WebAudio)

[![Web build and headless test](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml/badge.svg)](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml)

### Project status

- Web front-end (WASM) is running with:
- 3 voices, spatial audio (Web Audio + PannerNode)
- Lush ambient effects: global Convolver reverb and dark feedback Delay bus with per-voice sends and a master bus
- Mouse-driven FX: corner-based saturation (clean ↔ fizz) and opposite-corner delay emphasis; visuals have inertial swirl motion and click ripples
- Note-driven visuals use attack/release smoothing for organic response (no abrupt jumps)
- Start overlay to initialize audio (Click Start; canvas-click fallback)
- Drag voices in XZ plane; click to mute, Shift+Click reseed, Alt+Click solo
- Keyboard: A..F (root), 1..7 (mode), R (new sequence), T (random key+mode), Space (pause/resume), ArrowLeft/Right (tempo), ArrowUp/Down (volume), Enter (fullscreen)
- Starts at a lower default volume; use ArrowUp to raise or ArrowDown to lower
- Dynamic hint shows current BPM, paused, and muted state
- Rich visuals: instanced voice markers with emissive pulses, ambient waves background, post bloom/tonemap/vignette; optional analyser-driven spectrum dots

Note: Desktop UI has been removed to simplify the project; the focus is the web build.

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
- If audio does not start, click the Start overlay.
- Input coordinates: canvas UV origin is top-left (uv.y = 0 at top). Pointer-driven swirl and click ripple use this convention.

### Run (Web)

- Build: `npm run build:web`
- Serve: `node server.js` (serves `crates/app-web` with correct headers)
- Open: visit `http://localhost:8080`
- Dev shortcut: `npm run dev:web` then `npm run open:web`

Quick controls (browser):

- A..F: root • 1..7: mode • R: new seq • T: random key+mode • Space: pause/resume • ArrowLeft/Right: tempo • ArrowUp/Down: volume • Enter: fullscreen
- Click canvas: play a note; mouse position affects sound

### Pre-commit Check

- Run all checks and tests locally: `npm run check`
  - Rust: `cargo fmt --check`, `cargo clippy` (deny warnings), `cargo test` (workspace)
  - Web: build, serve, and execute the headless browser test

### Deploy (Cloudflare Workers)

This repo is configured to deploy via Cloudflare Workers; headers (COOP/COEP/CORP) are set in `worker.js`.

- Build: `npm run build:web`
- Deploy: `npx --yes wrangler deploy`
  - Config: `wrangler.toml` (assets at `crates/app-web`)
  - Worker adds the required headers and sensible cache-control for `.wasm`/`.js`/HTML

Controls in browser:

- Click Start to initialize audio (canvas click also works)
- Click canvas: play a note; mouse position affects sound
- Keys: A..F (root), 1..7 (mode), R (new sequence), T (random key+mode), Space (pause/resume), ArrowLeft/Right (tempo), ArrowUp/Down (volume), Enter (fullscreen)
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

<!-- Desktop run instructions removed -->

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
- `app-web`: web WASM front-end with WebGPU + WebAudio

### Docs

- Project Spec: `docs/SPEC.md`
- Project TODO: `docs/TODO.md`
- Audio Pipelines: `docs/diagrams/audio.md`
- Visual Pipelines: `docs/diagrams/visual.md`
