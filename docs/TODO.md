# Project TODO and Milestones

Remaining tasks aligned to `docs/SPEC.md`, focused only on incomplete and upcoming work.

## Audio Engine (Web)

- [ ] Optional AudioWorklet path (future)

## Microtonality (planned)

- [ ] Global microtonal detune (in cents) applied to all generated notes
  - [ ] Add `detune_cents: f32` to `EngineParams` (default 0.0)
  - [ ] Update pitch conversion to support fractional semitones: generalize `midi_to_hz(midi: f32)` to accept fractional values, or introduce `pitch_to_hz(semitones_from_a4: f32)`
  - [ ] Unit tests: verify 50¢ up/down relative to reference (A4) and round-trip expectations
- [ ] Microtonal scales support
  - [ ] Represent scales as `&'static [f32]` steps (semitones as floats) instead of `&'static [i32]`
  - [ ] Add example scales: 24-TET (quarter tones), 19-TET, 31-TET; optionally a simple JI pentatonic (ratios converted to cents)
  - [ ] Keep existing diatonic constants (IONIAN…LOCRIAN) by converting to `f32` for backward compatibility
  - [ ] Tests: ensure selection applies expected offsets and produces monotonically increasing Hz across steps

## Interaction & UI (Web)

- [ ] 3D in-scene icon controls replacing keyboard (post-v1)

### Keyboard – Microtonal Controls

- [ ] Bind keys for microtonal nudge
  - [ ] `,` decrease global detune by 50¢; `.` increase by 50¢ (hold Shift for 10¢ fine step)
  - [ ] `/` reset detune to 0¢
  - [ ] Update hint overlay to display current detune (e.g., “Detune: +50¢”)
- [ ] Add microtonal scale selection shortcuts (avoiding conflicts with 1–7 mode keys)
  - [ ] Proposed: `8` → 19-TET, `9` → 24-TET, `0` → 31-TET; repeat key to cycle variants if multiple
  - [ ] Update hint overlay to show active scale family (e.g., “Scale: 24-TET pentatonic”)

## Performance & Quality

- [ ] Profile; ensure steady 60 FPS on typical desktop GPUs
- [ ] Minimize JS↔Wasm transfers; reuse GPU buffers
- [ ] Cap polyphony / reuse oscillators; audit WebAudio lifetimes
  - [ ] Consider reducing WGSL noise/FBM cost or iterations if needed

## Code Hygiene and Refactors

- [ ] core: document public structs and functions
- [ ] core: introduce small newtype for `MidiNote` and typed Hz wrapper
- [ ] core: make `MusicEngine` scheduling grid configurable
- [ ] core: separate RNG/seeding from state to allow deterministic replay sessions
- [ ] app-web: reduce `lib.rs` size by moving init/wiring helpers into small submodules; keep existing `audio`, `render`, and `input` modules as-is (no new `ui` module)
- [ ] app-web: extract WebGPU pipeline builders

## Testing

- [ ] core: add property-based tests for `midi_to_hz` monotonicity and octave symmetry
- [ ] app-web: extend headless test to simulate tempo change and check hint reflects BPM
- [ ] app-web: add a check that clicking voices toggles mute text/icon state in the hint
