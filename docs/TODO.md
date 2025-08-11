# Project TODO and Milestones

This checklist tracks progress against the high-level plan in `docs/SPEC.md` and outlines the remaining work to reach a polished v1.

## Setup & Tooling

- [x] Single workspace crate (`app-web`) with internal `src/core` module (merged former `app-core`)
- [x] WebGPU initialization on web via `wgpu` v24 (no WebGL2 fallback)
- [x] Node dev server with proper COOP/COEP headers (`server.js`)
- [x] Headless web test (Puppeteer) scripted interactions
- [x] CI workflow to build web and run headless test
- [x] CI: handle Ubuntu 24.04 ALSA rename (install `libasound2t64` with fallback)
- [x] Add LICENSE file matching workspace `license = "MIT"`
- [x] Fill optional `description`/`repository` fields in crate manifests
- [x] Add README workflow badge (done) and keep CI green
- [x] Architecture diagrams for audio and visual pipelines (see `docs/diagrams/`)

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

### Microtonality (planned)

- [ ] Global microtonal detune (in cents) applied to all generated notes
  - [ ] Add `detune_cents: f32` to `EngineParams` (default 0.0)
  - [ ] Update pitch conversion to support fractional semitones: generalize `midi_to_hz(midi: f32)` to accept fractional values, or introduce `pitch_to_hz(semitones_from_a4: f32)`
  - [ ] Unit tests: verify 50¢ up/down relative to reference (A4) and round-trip expectations
- [ ] Microtonal scales support
  - [ ] Represent scales as `&'static [f32]` steps (semitones as floats) instead of `&'static [i32]`
  - [ ] Add example scales: 24-TET (quarter tones), 19-TET, 31-TET; optionally a simple JI pentatonic (ratios converted to cents)
  - [ ] Keep existing diatonic constants (IONIAN…LOCRIAN) by converting to `f32` for backward compatibility
  - [ ] Tests: ensure selection applies expected offsets and produces monotonically increasing Hz across steps

## Visual Engine (Web)

- [x] Instanced rendering of voice markers (circle mask, emissive pulse)
- [x] Audio-reactive pulses on note events
- [x] Attack/release smoothing of note-driven pulses to eliminate jittery jumps
- [x] Ambient visuals (ambient waves background with swirl and click ripples; optional analyser-driven spectrum dots)
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

### Keyboard – Microtonal Controls (planned)

- [ ] Bind keys for microtonal nudge
  - [ ] `,` decrease global detune by 50¢; `.` increase by 50¢ (hold Shift for 10¢ fine step)
  - [ ] `/` reset detune to 0¢
  - [ ] Update hint overlay to display current detune (e.g., “Detune: +50¢”)
- [ ] Add microtonal scale selection shortcuts (avoiding conflicts with 1–7 mode keys)
  - [ ] Proposed: `8` → 19-TET, `9` → 24-TET, `0` → 31-TET; repeat key to cycle variants if multiple
  - [ ] Update hint overlay to show active scale family (e.g., “Scale: 24-TET pentatonic”)

## Platform Notes

Desktop UI support has been removed to simplify the project and focus on the web build.

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

### Planned Refactors (incremental, no behavior changes)

- [x] core: extract helper methods for per-voice scheduling parameters
  - Rationale: improve readability in `schedule_step` by removing inline match blocks
  - Status: implemented as private helpers (`voice_trigger_probability`, `voice_octave_offset`, `voice_base_duration`) and rustdoc added; no API change
- [ ] core: document public structs and functions
  - Rationale: clarify responsibilities of `MusicEngine`, `EngineParams`, `VoiceConfig`, etc.
  - Status: partially done (added rustdoc to types and methods); continue across `constants.rs`/`state.rs`
- [ ] core: introduce small newtype for `MidiNote` and typed Hz wrapper
  - Rationale: make units explicit and reduce accidental misuse
  - Plan: add newtypes with From/Into impls; keep existing APIs to avoid breakage initially
- [ ] core: make `MusicEngine` scheduling grid configurable
  - Rationale: today hard-coded to eighth-note; allow division enum without changing defaults
  - Plan: add `grid_division: Division` to `EngineParams` with `Eighth` default; use in `tick`
- [ ] core: separate RNG/seeding from state to allow deterministic replay sessions
  - Rationale: enable capture/replay of seeds per voice for tests/demos
  - Plan: introduce `EngineRandom` struct; keep current methods as thin wrappers
- [ ] app-web: factor large `lib.rs` into modules (`audio`, `render`, `input`, `ui`)
  - Rationale: improve maintainability of a >2k LOC file
  - Plan: create `mod` submodules and move code in small PR-sized steps; keep exports stable
- [x] app-web: centralize DOM/hint/UI updates behind a tiny view model (initial)
  - Rationale: reduce ad-hoc DOM writes scattered in event handlers
  - Status: introduced `ui::refresh_hint_if_visible` and `set_hint_visibility` to unify updates; consider a lightweight `UiState` later if more fields accrue
- [x] app-web: small readability refactors in `lib.rs` (no behavior changes)
  - Rationale: reduce duplication and magic numbers
  - Status: added `hide_overlay()` helper, deduplicated pointer position set, and replaced magic numbers with `BASE_SCALE`/`SCALE_PULSE_MULTIPLIER`
- [x] app-web: centralize camera distance constant
  - Rationale: avoid duplication between picking, listener, and renderer
  - Status: added `CAMERA_Z` to `constants.rs` and updated imports in `frame.rs`, `lib.rs`, and `events/pointer.rs`
- [ ] app-web: extract WebGPU pipeline builders
  - Rationale: deduplicate pipeline/buffer setup for waves/post passes
  - Plan: create `pipeline.rs` helpers returning typed bundles; no functional changes
  - Status: partially done (post pipelines factored into helper in `render.rs`)
- [x] app-web: deduplicate pointer event wiring
  - Rationale: avoid drift by handling `pointermove`/drag/hover in one place (`events::wire_input_handlers`)
  - Impact: removed duplicate handler from `lib.rs`; behavior unchanged
- [x] app-web: centralize tuning constants in `constants.rs` and use in `frame.rs`
- [x] app-web: factor color texture creation and post blit/pipelines in `render.rs`
- [x] app-web: extract `render/targets.rs`, `render/post.rs`, `render/waves.rs`
  - State: moved targets, post pipeline/blit/bindgroup rebuild, and waves resources out of `render.rs`
- [x] app-web: overlay toggles CSS class via `classList` with style fallback
- [ ] app-web: split `events.rs` into `events/keyboard.rs` and `events/pointer.rs`

### Testing Enhancements

- [x] core: add unit tests for midi conversion, mute/solo, tempo effects, reseed determinism
- [ ] app-core: add property-based tests for `midi_to_hz` monotonicity and octave symmetry
- [ ] app-web: extend headless test to simulate tempo change and check hint reflects BPM
- [ ] app-web: add a check that clicking voices toggles mute text/icon state in the hint

## Testing & DX

- [x] Headless web test validates interactions and hint content
- [x] Add assertions: BPM change reflected; solo/mute state (logs/state)

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

### M4: Performance & QA

- Profiling and tuning for 60 FPS; audit WebAudio nodes, buffer reuse
- Expand headless tests; document manual steps

### M5: v1 Packaging

- LICENSE added; README finalized; hosting instructions
- Optional: tag v1.0.0 and publish demo

## Notes

- Per `docs/SPEC.md`, stay pure WebGPU (wgpu v24) and prioritize desktop web.
- UI remains minimalist; in-scene icon controls can follow in v1.1.
