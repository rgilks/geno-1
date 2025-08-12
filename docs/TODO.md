# Project TODO and S-Tier Roadmap

This document outlines the path from the current **A-** grade to **S-tier** status. Tasks are prioritized by impact and organized into clear milestones.

## 🏆 **Current Status: A- Grade**

### ✅ **Completed (Recent Improvements)**

- [x] G key support for complete musical alphabet (A-G)
- [x] Configurable voice parameters (no hardcoded values)
- [x] Enhanced browser testing with performance monitoring
- [x] Property-based testing for mathematical functions
- [x] Improved WebGPU error handling with user feedback
- [x] Comprehensive shader documentation
- [x] 31 tests passing with zero warnings

---

## 🚀 **S-Tier Roadmap: Core Features**

### **Phase 1: Microtonality System (HIGH IMPACT - Makes project unique)**

#### 1.1 Foundation

- [x] **Add microtonal detune support**
  - [x] Add `detune_cents: f32` to `EngineParams` (default 0.0, range ±200¢)
  - [x] Update `midi_to_hz()` to accept fractional MIDI values for cent precision
  - [x] Unit tests: verify 50¢ detune accuracy, round-trip expectations, extreme values
  - [x] Integration test: ensure detune affects all generated notes consistently

#### 1.2 Alternative Tuning Systems

- [ ] **Microtonal scales infrastructure**
  - [ ] Convert scale representation from `&'static [i32]` to `&'static [f32]` (semitones as floats)
  - [ ] Maintain backward compatibility: convert existing IONIAN…LOCRIAN constants
  - [ ] Add validation: ensure scales are monotonically increasing
- [ ] **Alternative tuning systems**
  - [ ] 19-TET (19-tone equal temperament): `&[0.0, 0.63, 1.26, 1.89, ...]`
  - [ ] 24-TET (quarter-tone system): `&[0.0, 0.5, 1.0, 1.5, 2.0, ...]`
  - [ ] 31-TET (extended equal temperament): `&[0.0, 0.39, 0.77, ...]`
  - [ ] Just Intonation pentatonic: `&[0.0, 2.04, 3.86, 7.02, 9.69, 12.0]` (ratios converted to cents)

#### 1.3 User Interface

- [x] **Keyboard controls for microtonality**
  - [x] `,` key: decrease global detune by 50¢ (Shift+`,` for 10¢ fine adjustment)
  - [x] `.` key: increase global detune by 50¢ (Shift+`.` for 10¢ fine adjustment)
  - [x] `/` key: reset detune to 0¢
  - [x] Update hint overlay: shows "Detune: ±N¢" and BPM/Scale
- [ ] **Scale selection shortcuts**
  - [ ] `8` key → 19-TET pentatonic
  - [ ] `9` key → 24-TET pentatonic
  - [ ] `0` key → 31-TET pentatonic
  - [ ] Repeat key press cycles through variants if multiple available
  - [ ] Visual feedback in hint overlay showing active tuning system

---

### **Phase 2: 3D Interactive UI (HIGH IMPACT - Revolutionary UX)**

#### 2.1 3D Control Objects

- [ ] **Replace keyboard shortcuts with 3D scene objects**
  - [ ] Play/pause orb: central floating sphere, color-coded state (green/red)
  - [ ] Tempo dial: ring around play/pause orb, drag to adjust BPM
  - [ ] Regenerate button: floating "refresh" icon, click to reseed all voices
  - [ ] Scale selector: floating geometric shapes, each representing a mode/tuning
- [ ] **Visual feedback system**
  - [ ] Hover effects: glow, scale up, color shift
  - [ ] Click animations: pulse, ripple effects
  - [ ] State indicators: visual cues for current mode, tempo, detune level

#### 2.2 Advanced Interaction

- [ ] **3D spatial voice mixing**
  - [ ] Enhance current drag system: visual voice objects in 3D space
  - [ ] Real-time position feedback: trails, connection lines to listener position
  - [ ] Distance-based volume visualization: size/brightness indicates audio level
  - [ ] Constraint visualization: show movement boundaries (drag radius)
- [ ] **Immersive control paradigms**
  - [ ] Voice solo/mute: click voice objects directly (no keyboard needed)
  - [ ] Per-voice effects: drag voices near effect zones to increase send levels
  - [ ] Visual mixing board: 3D representation of audio routing

---

### **Phase 3: Advanced Architecture (MEDIUM IMPACT - Professional quality)**

#### 3.1 Type Safety & Domain Modeling

- [ ] **Introduce strong types**
  - [ ] `MidiNote` newtype: prevents mixing MIDI values with other numbers
  - [ ] `Frequency` newtype: type-safe Hz values with validation
  - [ ] `Cents` newtype: microtonal offset type with bounds checking
  - [ ] `BPM` newtype: tempo type with realistic range validation (40-240)
- [ ] **Enhanced music engine**
  - [ ] Configurable scheduling grid: support 16th notes, triplets, dotted rhythms
  - [ ] Deterministic replay: separate RNG state from engine state
  - [ ] Voice probability curves: more sophisticated triggering patterns
  - [ ] Pattern memory: voices can "remember" and vary previous sequences

#### 3.2 Performance & Modularity

- [ ] **Code organization**
  - [ ] Extract `lib.rs` initialization into `src/init/` submodule
  - [ ] Create `src/pipeline/` for WebGPU pipeline builders
  - [ ] Modularize audio graph construction in `src/audio/graph.rs`
  - [ ] Document all public APIs with comprehensive examples
- [ ] **Performance optimization**
  - [ ] Implement AudioWorklet for sample-accurate timing
  - [ ] GPU buffer reuse: minimize allocation/deallocation
  - [ ] Oscillator pooling: cap polyphony, reuse WebAudio nodes
  - [ ] Profile and ensure consistent 60 FPS on mid-range GPUs

---

### **Phase 4: Polish & Advanced Features (MEDIUM IMPACT - Exceptional quality)**

#### 4.1 Enhanced Audio Engine

- [ ] **Advanced synthesis**
  - [ ] FM synthesis option: frequency modulation for richer timbres
  - [ ] Envelope shaping: configurable ADSR per voice
  - [ ] Voice filters: per-voice lowpass/highpass with cutoff automation
  - [ ] Advanced reverb: convolution with multiple impulse responses
- [ ] **Intelligent composition**
  - [ ] Markov chain melody generation: learn from user interactions
  - [ ] Harmonic analysis: ensure pleasant voice interactions
  - [ ] Rhythm complexity: polyrhythmic patterns across voices
  - [ ] Key modulation: gradual shifts between related keys

#### 4.2 Visual Excellence

- [ ] **Advanced rendering**
  - [ ] HDR bloom with configurable intensity
  - [ ] Particle systems: notes spawn visual particles
  - [ ] Shader-based audio visualization: waveform/spectrum displays
  - [ ] Dynamic lighting: voices cast colored light in 3D space
- [ ] **Responsive design**
  - [ ] Adaptive quality: reduce effects on lower-end hardware
  - [ ] Performance monitoring: real-time FPS display with auto-adjustment
  - [ ] Memory management: prevent WebGL context loss

---

### **Phase 5: Testing & Documentation Excellence (LOWER IMPACT - Demonstrates mastery)**

#### 5.1 Comprehensive Testing

- [ ] **Advanced unit tests**
  - [ ] Microtonal accuracy: verify cent-level precision across all tuning systems
  - [ ] Audio graph integrity: test WebAudio node connections and cleanup
  - [ ] Performance regression: automated benchmarking in CI
  - [ ] Cross-browser compatibility: test WebGPU across Chrome/Edge/Firefox
- [ ] **Enhanced browser testing**
  - [ ] UI interaction simulation: test 3D control objects
  - [ ] Audio-visual synchronization: verify timing between audio and visuals
  - [ ] Performance validation: fail CI if FPS drops below 45 on reference hardware
  - [ ] Accessibility: keyboard navigation, screen reader compatibility

#### 5.2 Documentation & Developer Experience

- [ ] **API documentation**
  - [ ] Comprehensive rustdoc for all public interfaces
  - [ ] Interactive examples: minimal working examples for each feature
  - [ ] Architecture diagrams: visual representation of system components
  - [ ] Performance guides: optimization tips and profiling instructions
- [ ] **User documentation**
  - [ ] Interactive tutorial: guided tour of features
  - [ ] Keyboard reference: comprehensive control documentation
  - [ ] Troubleshooting guide: common issues and solutions
  - [ ] Musical theory primer: explain microtonality and alternative tunings

---

## 🎯 **S-Tier Success Criteria**

### **Technical Excellence**

- [ ] Zero compilation warnings with strict linting
- [ ] 100% test coverage on core music logic
- [ ] Consistent 60 FPS on 75% of desktop hardware
- [ ] Memory usage under 100MB sustained
- [ ] WebAudio graph cleanup with no leaks

### **Feature Completeness**

- [ ] Full microtonality support (detune + alternative tunings)
- [ ] 3D interactive controls replacing all keyboard shortcuts
- [ ] Advanced synthesis options (FM, filters, envelopes)
- [ ] Professional-grade audio effects chain
- [ ] Intelligent generative composition

### **User Experience**

- [ ] Intuitive 3D interface discoverable without documentation
- [ ] Smooth performance across wide range of hardware
- [ ] Graceful degradation when WebGPU unavailable
- [ ] Comprehensive accessibility support
- [ ] Professional visual polish matching high-end audio software

### **Code Quality**

- [ ] Strong typing throughout (newtypes for domain concepts)
- [ ] Modular architecture supporting easy extension
- [ ] Comprehensive error handling with user-friendly messages
- [ ] Professional documentation and examples
- [ ] Exemplary Rust idioms and best practices

---

## 📊 **Implementation Priority**

1. **🚀 Phase 1 (Microtonality)** - Unique differentiator, high technical value
2. **🎮 Phase 2 (3D UI)** - Revolutionary user experience, high wow factor
3. **🏗️ Phase 3 (Architecture)** - Professional code quality, maintainability
4. **✨ Phase 4 (Polish)** - Exceptional quality, advanced features
5. **📚 Phase 5 (Testing/Docs)** - Demonstrates mastery, completeness

**Estimated effort**: 40-60 hours for full S-tier implementation
**Minimum S-tier**: Complete Phases 1-2 (microtonality + 3D UI)
**Maximum impact**: All phases for industry-leading web audio application

---

## 🎵 **Legacy Tasks (Lower Priority)**

### Audio Engine Enhancements

- [ ] Optional AudioWorklet path for improved timing precision
- [ ] Consider reducing WGSL noise/FBM cost or iterations if needed
- [ ] Cap polyphony / reuse oscillators; audit WebAudio lifetimes

### Code Cleanup

- [ ] app-web: reduce `lib.rs` size by moving init/wiring helpers into small submodules
- [ ] app-web: extract WebGPU pipeline builders
- [ ] Profile; ensure steady 60 FPS on typical desktop GPUs
- [ ] Minimize JS↔Wasm transfers; reuse GPU buffers

### Testing Completions

- [ ] app-web: extend headless test to simulate tempo change and check hint reflects BPM
- [ ] app-web: add a check that clicking voices toggles mute text/icon state in the hint
