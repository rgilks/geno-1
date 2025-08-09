# Project TODO and Milestones

This checklist tracks progress against the high-level plan in `docs/SPEC.md` and outlines the remaining work to reach a polished v1.

## Setup & Tooling

- [x] Workspace crates (`app-core`, `app-web`, `app-native`)
- [x] WebGPU initialization on web via `wgpu` v24 (no WebGL2 fallback)
- [x] Node dev server with proper COOP/COEP headers (`server.js`)
- [x] Headless web test (Puppeteer) scripted interactions
- [x] CI workflow to build web and run headless test
- [x] CI: handle Ubuntu 24.04 ALSA rename (install `libasound2t64` with fallback)
- [x] Add LICENSE file matching workspace `license = "MIT"`
- [x] Fill optional `description`/`repository` fields in crate manifests
- [x] Add README workflow badge (done) and keep CI green

## Audio Engine (Web)

- [x] Web Audio graph (`AudioContext`)
- [x] Per-voice `PannerNode` (spatialization) and `GainNode` envelope
- [x] Oscillator synthesis with per-voice waveform (sine/saw/triangle)
- [x] Generative scheduler at tempo (eighth-note grid), 3 voices
- [x] Controls: BPM, reseed voice/all, mute, solo (click/keys)
- [x] Distance attenuation via `PannerNode` with drag movement
- [x] Optional `AnalyserNode` to drive ambient visuals
- [x] Master bus with lush `ConvolverNode` reverb and dark feedback `DelayNode` bus with lowpass tone shaping; per-voice sends
- [ ] Optional AudioWorklet path (future)
- Notes: Master starts muted; Start overlay ensures gesture unlock. Pointer corners map to saturation/delay; click injects ripple into background waves.

## Visual Engine (Web)

- [x] Instanced rendering of voice markers (circle mask, emissive pulse)
- [x] Audio-reactive pulses on note events
- [x] Ambient visuals (ambient waves background with swirl and click ripples; optional analyser-driven spectrum dots)
- [ ] Optional camera orbit (toggle 'O') [removed in current build]
- [x] Sync listener orientation with camera
- [x] Visual polish (colors, easing, subtle glow, vignette)
- [x] Prefer SRGB surface format where available (e.g., BGRA8UnormSrgb)
- [x] Inertial pointer swirl (spring-damper) for water-like motion
  - [x] Post stack: bright-pass, separable blur, ACES tonemap, vignette, grain

## Interaction & UI (Web)

- [x] Ray picking (ray-sphere), hover highlight, XZ drag of voices
- [x] Click: mute; Shift+Click: reseed; Alt+Click: solo
- [x] Keyboard: R (reseed all), Space (pause), + / - (tempo), M (master mute)
  - [x] Start overlay (gesture) and default master mute
  - [x] Dynamic hint overlay shows BPM, paused, and muted state
- [ ] 3D in-scene icon controls replacing keyboard (post-v1)
- [x] Clamp drag radius to a sensible range to avoid losing objects
- [x] Mouse-driven FX mapping: corner-based saturation; opposite-corner delay
  - [x] Click/tap ripple expands in waves background

## Cross-Platform / Native

- [x] Native window via `winit` and rendering via `wgpu`
- [x] Basic native audio via `cpal` with envelopes
- [x] Map native audio to per-voice waveforms (sine/square/saw/triangle)
- [x] Stereo panning by X based on voice position (equal-power)
- [x] Native input parity (hover, drag, click)
  - [x] Subtle master saturation in native output

## Error Handling & UX

- [x] Graceful message if `navigator.gpu` not available (WebGPU unsupported)
- [x] Graceful message if `AudioContext` fails to initialize (permissions)

## Performance & Quality

- [ ] Profile; ensure steady 60 FPS on typical desktop GPUs
- [ ] Minimize JS↔Wasm transfers; reuse GPU buffers
- [ ] Cap polyphony / reuse oscillators; audit WebAudio lifetimes
  - [ ] Consider reducing WGSL noise/FBM cost or iterations if needed

## Code Hygiene

- [x] Resolve minor warnings in `app-web` (unused `mut`, unused `format` field)
- [x] Centralize color/theme constants and object sizes

## Testing & DX

- [x] Headless web test validates interactions and hint content
- [x] Add assertions: BPM change reflected; solo/mute state (logs/state)
- [x] Optional native smoke test (launch, render few frames, exit)
  - [x] Add unit tests for `app-core` mute/solo edge-cases with reseed & tempo changes

## Deployment

- [x] Add hosting instructions (Cloudflare Workers)

- [x] Optional: production server/worker if needed
- [ ] Document `wrangler` env and cache headers expectations for `.wasm`/`.js`

---

## Milestones

### M1: Solid Web Prototype (current)

- 3 voices with spatial audio, reactive visuals, drag interactions
- Keyboard and hint overlay for control (mute/tempo)
- CI green with headless test

Status: In progress — core features completed; polish pending

### M2: Visual polish and optional analysis-driven effects

- Add analyser-driven ambient visuals (spectrum/particles) [optional]
- Add mild glow/color tuning in WGSL; subtle camera motion

### M3: Native parity improvements

- Stereo panning by X and per-voice waveforms in native
- Basic input parity (drag, mute/reseed/solo)

### M4: Performance & QA

- Profiling and tuning for 60 FPS; audit WebAudio nodes, buffer reuse
- Expand headless tests; document manual steps

### M5: v1 Packaging

- LICENSE added; README finalized; hosting instructions
- Optional: tag v1.0.0 and publish demo

## Notes

- Per `docs/SPEC.md`, stay pure WebGPU (wgpu v24) and prioritize desktop web. Native parity progresses where simple.
- UI remains minimalist; in-scene icon controls can follow in v1.1.
