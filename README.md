# Geno-1 : Generative Music Visualizer (Rust + WebGPU + WebAudio)

[![Web build and headless test](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml/badge.svg)](https://github.com/rgilks/geno-1/actions/workflows/web-ci.yml)

### Project status

- Web front-end (WASM) is running with:
- 3 voices, spatial audio (Web Audio + PannerNode)
- Lush ambient effects: global Convolver reverb and dark feedback Delay bus with per-voice sends and a master bus
- Mouse-driven FX: corner-based saturation (clean ↔ fizz) and opposite-corner delay emphasis; visuals have inertial swirl motion and click ripples
- Note-driven visuals use attack/release smoothing for organic response (no abrupt jumps)
- Start overlay to initialize audio (Click Start; canvas-click fallback)
- Keyboard: A..F (root), 1..7 (mode), R (new sequence), T (random key+mode), Space (pause/resume), ArrowLeft/Right (tempo), ArrowUp/Down (volume), Enter (fullscreen)
- Starts at a lower default volume; use ArrowUp to raise or ArrowDown to lower
- Dynamic hint shows current BPM, paused, and muted state
- Rich visuals: instanced voice markers with emissive pulses, ambient waves background, post bloom/tonemap/vignette; optional analyser-driven spectrum dots

### Demo

- Local: see Run (Web) below. After `npm run dev`, open `http://localhost:8080`.
- Hosted: [https://geno-1.tre.systems/](https://geno-1.tre.systems/)

### Requirements

- Node 20+
- Rust (stable, 2021 edition)
- wasm-pack (install: `curl -sSfL https://rustwasm.github.io/wasm-pack/installer/init.sh | sh`)
- Desktop browser with WebGPU enabled

Notes:

- WebGL fallback is intentionally avoided; WebGPU is required.
- If audio does not start, click the Start overlay.
- Input coordinates: canvas UV origin is top-left (uv.y = 0 at top). Pointer-driven swirl and click ripple use this convention.

### Run

- `npm run dev` (builds, serves at http://localhost:8080, and opens the browser)

Quick controls (browser):

- A..F: root • 1..7: mode • R: new seq • T: random key+mode • Space: pause/resume • ArrowLeft/Right: tempo • ArrowUp/Down: volume • Enter: fullscreen
- Click canvas: play a note; mouse position affects sound

### Pre-commit Check

- Run all checks and tests locally: `npm run check`
  - Rust: `cargo fmt --check`, `cargo clippy` (deny warnings), `cargo test` (workspace)
  - Web: build, serve, and execute the headless browser test

## Git hooks (local safety checks)

This repo uses native Git hooks in `.githooks` (no Husky dependency). Enable them once per clone:

```
git config core.hooksPath .githooks
```

Or run the convenience script (also ensures hooks are executable):

```
npm run setup
```

Hooks provided:

- `pre-commit`: runs fast Rust checks (`npm run check:rust`) to keep commits quick
- `pre-push`: runs the full project check (`npm run check`) before code leaves your machine

The full check enforces Rust fmt/clippy, runs unit tests, builds the web bundle, serves it, and executes the headless Puppeteer test. If any step fails, Git aborts the commit/push.

### Deploy (Cloudflare Workers)

This repo is configured to deploy via Cloudflare Workers; cache-control headers for static assets are set in `worker.js`.

- Build: `npm run build`
- Deploy: `npx --yes wrangler deploy`
  - Config: `wrangler.toml` (assets directory is `dist/`)
  - Build populates `dist/` with only production runtime files: `index.html` and `pkg/{app_web.js, app_web_bg.wasm, env.js}`
- Worker sets cache-control headers for `.wasm`/`.js`/HTML

Controls in browser:

- Click Start to initialize audio (canvas click also works)
- Click canvas: play a note; mouse position affects sound
- Keys: A..F (root), 1..7 (mode), R (new sequence), T (random key+mode), Space (pause/resume), ArrowLeft/Right (tempo), ArrowUp/Down (volume), Enter (fullscreen)
- Mouse position maps to master saturation and delay; moving the pointer leaves a “water-like” trailing swirl in visuals

Headless test:

- `npm run ci` builds, serves, and runs a Puppeteer test locally

### Continuous Integration

- GitHub Actions workflow runs on push/PR to main:
  - Builds the web bundle and executes the headless browser test
  - On push to `main`, deploys to Cloudflare Workers via Wrangler
  - Requires repo secrets: `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
  - Workflow file: `.github/workflows/ci.yml`
  - CI tolerates missing WebGPU in headless by skipping engine-coupled assertions

<!-- Desktop run instructions removed -->

### Live Demo

- Use the hosted app: [`https://geno-1.tre.systems/`](https://geno-1.tre.systems/)

Notes:

- Keeping screenshots/GIFs up to date is intentionally avoided; refer to the live app instead.

### Workspace crates

- `app-core`: shared music generation and state
- `app-web`: web WASM front-end with WebGPU + WebAudio
  - `src/render/targets.rs`: HDR/bloom textures create/recreate
  - `src/render/post.rs`: post pipelines, uniforms, blit, bind-group rebuild
  - `src/render/waves.rs`: waves pass uniforms and pipeline bundle
  - `src/render.rs`: orchestrates passes using the render/\* helpers

### Docs

- Project Spec: `docs/SPEC.md`
- Project TODO: `docs/TODO.md`
