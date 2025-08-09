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
- [ ] Optional `AnalyserNode` to drive ambient visuals
- [ ] Optional AudioWorklet path (future)

## Visual Engine (Web)

- [x] Instanced rendering of voice markers (circle mask, emissive pulse)
- [x] Audio-reactive pulses on note events
- [ ] Ambient visuals (particles/spectrum bars/lights)
- [ ] Optional camera orbit or mouse-look; sync listener orientation
- [ ] Visual polish (colors, easing, subtle glow)
- [x] Prefer SRGB surface format where available (e.g., BGRA8UnormSrgb)

## Interaction & UI (Web)

- [x] Ray picking (ray-sphere), hover highlight, XZ drag of voices
- [x] Click: mute; Shift+Click: reseed; Alt+Click: solo
- [x] Keyboard: R (reseed all), Space (pause), + / - (tempo), M (master mute)
  - [x] Start overlay (gesture) and default master mute
  - [x] Dynamic hint overlay shows BPM, paused, and muted state
- [ ] 3D in-scene icon controls replacing keyboard (post-v1)
- [x] Clamp drag radius to a sensible range to avoid losing objects

## Cross-Platform / Native

- [x] Native window via `winit` and rendering via `wgpu`
- [x] Basic native audio via `cpal` with envelopes
- [ ] Map native audio to per-voice waveforms (currently sine only)
- [ ] Stereo panning by X based on voice position
- [ ] Native input parity (hover, drag, click)

## Error Handling & UX

- [x] Graceful message if `navigator.gpu` not available (WebGPU unsupported)
- [x] Graceful message if `AudioContext` fails to initialize (permissions)

## Performance & Quality

- [ ] Profile; ensure steady 60 FPS on typical desktop GPUs
- [ ] Minimize JS↔Wasm transfers; reuse GPU buffers
- [ ] Cap polyphony / reuse oscillators; audit WebAudio lifetimes

## Code Hygiene

- [x] Resolve minor warnings in `app-web` (unused `mut`, unused `format` field)
- [ ] Centralize color/theme constants and object sizes

## Testing & DX

- [x] Headless web test validates interactions and hint content
- [ ] Add assertions: BPM change reflected; solo/mute state (logs/state)
- [ ] Optional native smoke test (launch, render few frames, exit)

## Deployment

- [x] Add hosting instructions (Cloudflare Workers)
- [ ] Optional: GitHub Pages workflow to publish `crates/app-web` artifacts (requires headers)
- [x] Optional: production server/worker if needed

---

## Milestones

### M1: Solid Web Prototype (current)

- 3 voices with spatial audio, reactive visuals, drag interactions
- Keyboard and hint overlay for control
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
