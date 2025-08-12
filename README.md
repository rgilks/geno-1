# Geno-1: Generative Music Visualizer (Rust + WebGPU + WebAudio)

[![CI](https://github.com/rgilks/geno-1/actions/workflows/ci.yml/badge.svg)](https://github.com/rgilks/geno-1/actions/workflows/ci.yml)

<div align="center">
 <img src="/docs/screenshot.png" alt="geno-1 Screenshot" width="626" />
  <br />
  <a href='https://ko-fi.com/N4N31DPNUS' target='_blank'><img height='36' style='border:0px;height:36px;' src='https://storage.ko-fi.com/cdn/kofi2.png?v=6' border='0' alt='Buy Me a Coffee at ko-fi.com' /></a>
  <hr />
</div>

### Project status

- Web front-end (WASM) is running with:
- 3 voices, spatial audio (Web Audio + PannerNode)
- Lush ambient effects: global Convolver reverb and dark feedback Delay bus with per-voice sends and a master bus
- Mouse-driven FX: corner-based saturation (clean ↔ fizz) and opposite-corner delay emphasis; visuals have inertial swirl motion and click ripples
- Note-driven visuals use attack/release smoothing for organic response (no abrupt jumps)
- Start overlay to initialize audio (Click Start; canvas-click fallback)
- Keyboard: A..G (root), 1..7 (mode), R (new sequence), T (random key+mode), Space (pause/resume), ArrowLeft/Right (tempo), ArrowUp/Down (volume), Enter (fullscreen)
- Starts at a lower default volume; use ArrowUp to raise or ArrowDown to lower
- Dynamic hint shows current BPM, paused, and muted state
- Rich visuals: voice-reactive wave displacement, ambient waves background, post bloom/tonemap/vignette; optional analyser-driven spectrum dots

### Demo

- Local: see Run (Web) below. After `npm run dev`, open `http://localhost:8787`.
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

- `npm run dev` (builds, serves at http://localhost:8787, and opens the browser)

Additional scripts:

- `npm run clean` (removes build artifacts)
- `npm run nuke` (full reset: removes node_modules, reinstalls, and runs dev)
- `npm run deps` (check for dependency updates)
- `npm run deps:update` (update dependencies and run nuke)

Quick controls (browser):

- Click Start to initialize audio (canvas click also works)
- Click canvas: play a note; mouse position affects sound
- Keys: A..G (root), 1..7 (mode), R (new sequence), T (random key+mode), Space (pause/resume), ArrowLeft/Right (tempo), ArrowUp/Down (volume), Enter (fullscreen)
- Mouse position maps to master saturation and delay; moving the pointer leaves a “water-like” trailing swirl in visuals

### Pre-commit Check

- Run all checks and tests locally: `npm run check`
  - Rust: `cargo fmt --check`, `cargo clippy` (deny warnings), `cargo test` (workspace)
  - Web: build, serve, and execute the headless browser test

### Git hooks (local safety checks)

This repo uses native Git hooks in `.githooks` (no Husky dependency). Enable them once per clone:

```bash
git config core.hooksPath .githooks
```

Or run the convenience script (also ensures hooks are executable):

```bash
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

#### Build cache-busting

- The build generates `pkg/env.js` with a `version` derived from the current git commit (short SHA, with CI env fallbacks). `index.html` appends `?v=<version>` to the dynamic import of `app_web.js` to ensure deterministic cache busting across deploys.

Headless test:

- `npm run ci` builds, serves, and runs a Puppeteer test locally

### Continuous Integration

- GitHub Actions workflow runs on push/PR:
  - Builds the web bundle and executes the headless browser test
  - On push to `main`, deploys to Cloudflare Workers via Wrangler
  - Requires repo secrets: `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
  - Workflow file: `.github/workflows/ci.yml`
  - CI tolerates missing WebGPU in headless by skipping engine-coupled assertions

### Live Demo

- Use the hosted app: [`https://geno-1.tre.systems/`](https://geno-1.tre.systems/)

Notes:

- Keeping screenshots/GIFs up to date is intentionally avoided; refer to the live app instead.

### Project structure

- `app-web`: single WASM crate with WebGPU + WebAudio
  - `src/core/`: music generation and shared state (formerly `app-core` crate)
  - `src/render/targets.rs`: HDR/bloom textures create/recreate
  - `src/render/post.rs`: post pipelines, uniforms, blit, bind-group rebuild
  - `src/render/waves.rs`: waves pass uniforms and pipeline bundle
  - `src/render.rs`: orchestrates passes using the render/\* helpers

### Docs

- Project Spec: `docs/SPEC.md`
- Project TODO: `docs/TODO.md`
