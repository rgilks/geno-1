# Generative 3D Music Visualizer – System Specification

## Project Overview

This project is an **interactive generative music visualizer** built with Rust, WebAssembly (Wasm), and WebGPU. It produces evolving musical sequences (melodies and harmonies generated algorithmically) and visualizes them in a 3D scene in real-time. The system supports **polyphonic** audio (multiple simultaneous sound voices) and arranges these sound sources in a virtual 3D space so that users experience spatial audio. The 3D visuals react dynamically to the music, providing an immersive audio-visual experience.

Users can **influence and interact** with the generative music without manually composing it. The interface is subtle and minimalistic – a hint overlay shows status and keys; primary controls are embedded in-scene and via keyboard. The primary target platform is **desktop web browsers** supporting WebGPU (no WebGL fallback by design). Mobile is not a focus.

### Current Capabilities (v1 prototype)

- 3 generative voices (sine/saw/triangle) with scale-constrained pitches (C major pentatonic by default), scheduler on an eighth-note grid
- Web Audio graph with per-voice `PannerNode` and master reverb/delay buses; starts muted with Start overlay; gesture unlock required by browsers
- Visuals: ambient waves background with voice-reactive displacement, pointer swirl and click ripples, post-processing (bright pass, blur, ACES tonemap, vignette, grain)
- Planned microtonality: global detune in cents and additional microtonal scale families (19-TET, 24-TET, 31-TET); keyboard shortcuts for detune and scale selection

## Goals and Use Cases

- **Generative Music Creation:** Continuously generate random or procedural musical sequences. The music should have multiple layers (voices) playing together (polyphony) to create rich soundscapes.

- **Spatial Audio Experience:** Position different sound voices in 3D space. As the listener (user) moves or as sound sources pan, the audio should reflect positional changes (e.g. volume or panning changes based on distance and direction).

- **Real-Time 3D Visualization:** Render a 3D scene that _visually represents_ the music in real-time. Visual elements (shapes, particles, lights, etc.) should react to musical features – e.g. pulsating with the beat, changing color or form with melody or intensity.

- **User Interaction:** Allow the user to influence the generation and playback:

  - The user can trigger changes in the music (e.g. regenerate a melody or all sequences, change the active scale/tonality, adjust tempo).
  - The user can manipulate sound sources or visual elements (e.g. moving a sound object in space to change its spatial audio effect, toggling a voice on/off).
  - The interactions should be intuitive _without traditional UI widgets_. Controls might be 3D objects or icons in the scene that the user clicks or drags.

- **Immersive UI/UX:** Provide a minimalist interface that blends with the visualization:

  - Controls are **part of the scene** (for example, floating icons or objects) rather than HTML buttons.
  - No text labels on controls (using tooltips or intuitive symbols if needed). The design should encourage exploration – e.g., an icon might hint its function via shape (🔄 for regenerate, ⏯ for play/pause, etc.) or by subtle animation.
  - The overall aesthetic is clean and not cluttered; UI elements do not distract from the visualizer but complement it.

- **Platform Focus:** Prioritize the web browser implementation.

## Technical Stack and Platform Constraints

- **Rust + WebAssembly (WASM):** The core application will be written in Rust and compiled to WebAssembly for running in browsers. Rust ensures high performance and reliability (important for real-time audio and graphics), and WASM allows it to run in web environments.

- **WebGPU (via WGPU crate):** All rendering will use the modern WebGPU API for GPU-accelerated graphics. We will use Rust’s [`wgpu`](https://github.com/gfx-rs/wgpu) library as an abstraction over WebGPU. This allows writing the graphics code once and running it on WebGPU in the browser and on Vulkan/Metal/DirectX backends natively. WebGPU provides the performance needed for complex 3D visuals in a browser environment, albeit with still limited browser/device support (hence focusing on desktop).

- **Audio:** Audio generation and output relies on the Web Audio API in the browser. The Web Audio graph uses per-voice `OscillatorNode` → `GainNode` envelope → `PannerNode` for spatialization, with a master `ConvolverNode` reverb and a dark feedback `DelayNode` bus (with tone shaping) fed by per-voice sends.

- **Windowing:** In the browser, the "window" is an HTML `<canvas>` with WebGPU context.

- **No External Engine:** We will not use heavy game engines or frameworks (like Unity, Unreal, or even a full engine like Bevy) because we want tight control over using Rust/WASM/WebGPU directly. The implementation will be mostly from scratch or with lightweight libraries:

  - Graphics: `wgpu` (and possibly math libraries like `glam` for vector math, if needed).
  - Audio: Web Audio via `web_sys` (and possibly an audio thread or `AudioWorklet` for smooth audio scheduling).
  - We may use smaller utility crates (for example, `rand` for randomness, `serde` if any config, etc.), but the core logic is custom.

- **Browser Compatibility:** The application targets browsers with WebGPU enabled; WebGL fallback is intentionally avoided. A Start overlay handles user gesture unlock for audio.

## System Architecture

The system is composed of three main subsystems:

1. **Audio Engine** – music generation (notes/sequences) and sound output (synthesis and spatial audio).
2. **Visual Engine** – 3D rendering using WebGPU, including ambient waves background and a post-processing stack (bright pass, separable blur, ACES tonemap, vignette, grain). The renderer is structured with small modules: `render/targets.rs` (offscreen targets), `render/waves.rs` (fullscreen waves pass), and `render/post.rs` (post pipelines, uniforms, blits, bind groups).
3. **Interaction & UI Module** – user input (mouse, keyboard) with a minimalist hint overlay; interactive controls embedded in the 3D scene.

These components will run simultaneously and communicate in real-time. The application runs as a single crate (`app-web`) with an internal `src/core` module (formerly the `app-core` crate). The app uses requestAnimationFrame in the browser to update both audio and visuals continuously:

- On each update tick (e.g. \~60 times per second for visuals, and audio scheduling in smaller increments), it will compute any state changes (notes to play, visual changes) and render a new frame.
- The audio engine might run on its own timing separate from the graphics frame rate (since audio needs steady timing). We will utilize timing facilities of Web Audio (e.g. scheduling notes with precise timing in the AudioContext) to ensure audio does not stutter even if graphics frame rate fluctuates.

Inter-component communication:

- The **Audio Engine** can send data or events to the Visual Engine (for example, if a note plays, it can notify the visuals to trigger an animation). This can be done via simple shared state or event queues. Because both run in the same Rust/WASM module, they can share memory/state. (If using an AudioWorklet, communication might need message passing to main thread, but we can possibly run much of generation on main thread and use the AudioContext scheduling to avoid heavy work in the worklet.)
- The **Interaction module** will translate user inputs into actions that affect the Audio and Visual engines. For example, a user clicking a certain object may cause the Audio Engine to mute a voice and Visual Engine to change that object’s appearance.

Below, we detail each subsystem:

### 1. Audio Engine

**Overview:**
The audio engine is responsible for producing continuous music with multiple voices, and for outputting the sound with spatial effects. It does _not_ rely on pre-recorded tracks; instead it generates notes and tones algorithmically (procedurally). Users can influence the parameters of this generation in real-time.

#### Audio Pipeline

```mermaid
graph TD
  %% Audio pipeline (Web)
  subgraph "Music Generation & Shared State"
    A["MusicEngine\n(eighth-note scheduler, BPM, scale)"]
    B["Shared Voice State\npositions • mute/solo"]
    A --> C["NoteEvent(s)\nvoice, freq, velocity, start, duration"]
    B -.-> D["Voice Positions\n(x,y,z)"]
  end

  %% Web Audio path
  subgraph "Audio"
    %% Per-note source and per-voice strip
    C --> N1["OscillatorNode\n(per-note, per-voice waveform)"]
    N1 --> V1["GainNode (env)\n(attack/release)"]
    V1 --> VG["Voice Gain"]
    VG --> P["PannerNode\n(HRTF positional)"]
    D -.-> P
    P --> M["Master Gain bus"]

    %% Pre-panner effect sends (per voice)
    V1 -.-> DS["Delay Send"]
    V1 -.-> RS["Reverb Send"]
    DS --> DI["Delay In"]
    RS --> RI["Reverb In"]

    %% Delay bus with feedback and tone shaping
    DI --> DL["DelayNode"]
    DL --> LT["Biquad Lowpass\n(tone)"]
    LT --> DF["Delay Feedback"]
    DF --> DL
    LT --> DW["Delay Wet"]
    DW --> M

    %% Reverb bus
    RI --> CV["ConvolverNode\n(impulse)"]
    CV --> RW["Reverb Wet"]
    RW --> M

    %% Master saturation and output (wet/dry mix)
    M --> SP["Sat Pre Gain"]
    SP --> WS["WaveShaperNode\n(arctan)"]
    WS --> SW["Sat Wet"]
    SW --> DEST["AudioContext.destination"]
    M --> SD["Sat Dry"]
    SD --> DEST

    %% Optional analyser tap (for visuals only)
    M -.-> AN["AnalyserNode (optional)"]
  end
```

**Key Responsibilities:**

- Maintain **three voices** with distinct timbres (sine/saw/triangle) and roles (bass/mid/high).
- **Generative Music Algorithm:** Eighth-note grid scheduler per voice with probabilities, scale-constrained pitches (default C major pentatonic), and per-voice octave ranges; reseeding randomizes sequences.

  - _Random within constraints:_ e.g., define a musical scale (set of allowed pitches) and have each voice pick random notes from that scale. Ensure some rhythmic structure (like a fixed tempo and grid, e.g., 120 BPM with 8 beats per measure, etc., then randomly decide to play or not play a note on each subdivision for a pattern).
  - _Algorithmic composition:_ for more interesting output, techniques like Markov chains, cellular automata, or simple procedural rules can be used to vary the melody. However, initially a simpler random or loop-based pattern generator can be sufficient.
  - The sequences should **evolve over time** (to avoid being too repetitive). For example, every few measures, introduce a chance to change a note or generate a new pattern so that the music is continuously refreshing.
  - The system should ensure the music remains consonant/pleasant: using a preset scale (like a pentatonic or diatonic scale) avoids dissonant random notes. We might choose a default scale (e.g., C major pentatonic) or allow the scale to be changed by the user.

- **Polyphony:** All voices play simultaneously in synchronization, creating polyphonic texture. The engine should handle scheduling notes such that multiple sounds can overlap. For instance, Voice A might sustain a note while Voice B triggers several short notes, etc.
- **Sound Synthesis:** Actually generating the sound for each note:

  - In the browser, use the Web Audio API. For example, for each note event, create an `OscillatorNode` (or reuse a small pool of oscillators) with a chosen waveform (sine, square, etc.) and frequency corresponding to the note’s pitch. Connect it through any effect nodes (if desired, e.g. a GainNode for volume envelope, maybe a BiquadFilterNode for tone shaping) then into a PannerNode for spatialization, and finally to the AudioContext destination.
  - Alternatively, use an `AudioBufferSourceNode` with precomputed waveforms or samples, but Oscillator is simpler for pure tones. Because this is generative, using basic waveforms might suffice, or we could generate more complex timbres (like blending waves or using FM synthesis) if needed for richness.
  - Each voice can use a different waveform or effect to distinguish their sound (e.g., Voice A uses a low sine wave (bass), Voice B a sawtooth (for a bright lead), Voice C maybe a soft triangle wave for pad). The choice can be adjusted as part of design, but ensure differences so user can aurally tell them apart.
  - Implement volume envelope (attack/release) so notes aren’t clicks: e.g., use GainNode to fade in/out notes over a few milliseconds.

- **Spatial Audio:** Position each voice’s sound in 3D:

  - For each voice, create a **PannerNode** in Web Audio and set its 3D position coordinates. The audio source node for that voice feeds into this panner. The AudioListener is attached to the camera or viewer position (in a typical way, AudioContext.listener is the listener).
  - By adjusting a voice’s PannerNode position, the sound will pan between left/right and attenuate with distance, giving a sense of space. We can initialize each voice at a default position (e.g., spread them out a bit in the scene – one to the left, one to the right, one center or back, etc.).
  - If the user moves the camera or if we allow user to move the sound sources (dragging objects), update the PannerNode positions accordingly. Use an appropriate `distanceModel` (probably “linear” or “inverse”) so that distance affects volume naturally, and maybe set maxDistance so sounds don’t completely disappear if far.
  - Ensure that spatialization is subtle enough to be pleasant – for example, not panning extremely hard unless intended. The goal is immersive sound, not distraction.

- **Timing and Scheduling:** The audio engine should run on a stable timing mechanism:

  - Use the AudioContext’s time for scheduling. For example, you can schedule oscillator start/stop times in the future. We might have a function that continuously schedules a little bit ahead (say one bar of music ahead) so that even if the main thread is busy rendering, the audio plays smoothly (Web Audio can handle scheduled events in its own thread).
  - Alternatively, use an **AudioWorklet** to generate audio continuously via script if sample-level control is needed. But using the built-in oscillator nodes with scheduled timings will likely suffice and is simpler.
  - The **tempo** of the music is adjustable via keyboard (`+`/`-`). Default BPM is 110.

- **Parameter Controls:**

  - Per-voice mute/solo via click/Alt+Click; Shift+Click reseeds a voice; `R` reseeds all; `Space` toggles pause;
  - These changes should take effect seamlessly: if user changes tempo, new notes should align to the new tempo. If user regenerates, the old pattern can either stop immediately or finish the measure then switch, depending on desired effect.

- **Polyphony Performance:** The audio system must handle multiple simultaneous sounds efficiently. Using the Web Audio API’s built-in nodes is quite efficient in the browser. But we must be cautious not to create too many nodes unbounded (which could use too much CPU). Reusing nodes or limiting polyphony per voice (e.g., each voice usually just plays one note at a time in our design) helps. If chords are needed, that’s essentially multiple voices.
- **Audio Reactivity Data:** Provide data to the Visual Engine about the sound for visualization:

  - We can compute or retrieve amplitude or frequency information. For example, use an `AnalyserNode` in Web Audio to get waveform or frequency spectrum data in real-time. That can be passed to the visual part (e.g., via JS interop) or shared state for driving visual effects.
  - Or simply, since our engine triggers notes, we know when a note starts and its volume/pitch – we can send an event like “Voice1 played note C4 with velocity X” to the visual system. This may even be easier than using an FFT, since we have discrete musical events.
  - A combination is possible: use events for note onsets and an analyser for overall volume/frequency for smooth continuous visualization.

**Summary of Audio Engine Implementation (Browser):**
Rust (wasm) code will use `web_sys::AudioContext` to set up an audio graph. It will likely:

1. Create `AudioContext` and retrieve `AudioContext.listener` (set listener position/orientation tied to camera).
2. For each voice:

   - Create an `OscillatorNode` or a mechanism to produce sound (maybe create on the fly per note).
   - Create a `GainNode` for volume envelope control.
   - Create a `PannerNode` for spatialization, set its initial position.
   - Connect Oscillator -> Gain -> Panner -> AudioContext.destination.

3. Start a loop (maybe using `window.setInterval` or animation frame callbacks) to schedule notes: e.g., every quarter-beat, decide if a note should play on each voice in the next beat and schedule oscillator start/stop accordingly.

   - Alternatively, use an AudioWorklet: implement a custom processor that outputs a mix of oscillators. But initial approach can stick to high-level nodes.

4. Respond to user input by adjusting nodes (e.g., change Panner position when user drags source; change GainNode.gain for volume sliders; when regenerating, pick new random notes for upcoming bars).
5. Use `AnalyserNode` (optional) by inserting it in the chain (e.g., at the master output) to get audio data for visualizations. Or maintain state of recent notes.

#### References

- [Web Audio API overview (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API)
- [AudioContext (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/AudioContext)
- [AudioParam automation (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/AudioParam)
- [OscillatorNode (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/OscillatorNode)
- [GainNode (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/GainNode)
- [PannerNode (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/PannerNode) and [AudioListener (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/AudioListener)
- [ConvolverNode (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/ConvolverNode)
- [DelayNode (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/DelayNode)
- [BiquadFilterNode (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/BiquadFilterNode)
- [WaveShaperNode (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/WaveShaperNode)
- [AnalyserNode (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/AnalyserNode)

### 2. Visual Engine (3D Graphics)

**Overview:**
The visual engine renders a real-time 3D scene using WebGPU. The visuals are tightly coupled to the audio – essentially providing a **visual representation of the music** as it plays. Think of it as a music visualizer but in a three-dimensional, possibly interactive form.

#### Visual Pipeline

```mermaid
graph TD
  %% Visual pipeline
  subgraph "Input & Audio Reactivity"
    I1["Pointer / Keyboard events"]
    I2["NoteEvent Pulses\n(per voice velocity → pulse)"]
    I3["Shared Voice State\npositions • colors • pulses"]
    I1 -->|drag, click, keys| I3
    I2 -->|on note| I3
  end

  subgraph "Rendering"
    R0["WGPU Device + Surface"]
    R1["Ambient Waves (waves.wgsl)\nFullscreen pass; swirl, voice displacement, ripple"]
    R2["Post Stack (post.wgsl)\nBright pass → Separable blur → Composite\nACES tonemap, vignette, grain"]
    R3["Swapchain Present"]
    I3 -.->|uniforms per frame| R1
    R1 --> R2
    R2 --> R3
  end

  subgraph "Browser specifics"
    WGPU["WebGPU on Canvas"]
    AC["AudioListener tied to camera"]
    R0 --> WGPU
    I3 -.-> AC
  end
```

**Graphics Setup:**

- Use the `wgpu` crate in Rust to interface with WebGPU. In the browser, `wgpu` will create a context that maps to the HTML Canvas’s WebGPU context (via `wgpu::Surface` acquired from the canvas).
- We will define a **rendering pipeline** with shaders for drawing our scene. Likely, we'll use one or more **shaders (WGSL)** to render the shapes and effects.
- Basic steps:

  - Initialize WebGPU (request adapter, device, create swap chain surface for the canvas).
  - Load/create geometry for visual elements (e.g., vertex buffers for shapes, maybe use simple primitives like spheres, cubes, or custom shapes).
  - Create uniform buffers or textures for any dynamic data (like camera matrices, audio-driven values).
  - Write WGSL shaders for vertex and fragment stages to draw objects, possibly with properties (color, size) that we can change per frame.
  - Each animation frame, update the scene (positions, sizes, colors of objects) based on the latest audio state, then encode commands and submit to GPU to render the frame.

**Scene and Visual Elements:**
What the user sees:

- **Voice Influence on Waves:** Voice positions influence the wave patterns through displacement and proximity effects, creating golden highlights and wave distortions around each voice location.
- **Ambient Waves Background:** A fullscreen pass (see `waves.wgsl`) renders layered ribbons with pointer-driven swirl displacement, per-voice influence, and click/tap ripple propagation.
- **Post-processing:** A post stack (see `post.wgsl`) performs bright pass, separable blur, ACES tonemap, vignette, subtle hue warp, and film grain.
- **Camera:** Fixed view; the `AudioListener` tracks the camera to maintain spatial consistency.

**Visual Reactivity Implementation:**

- The Visual Engine will get data from the Audio Engine about what’s happening. We can implement a small messaging or state-sharing:

  - For example, maintain a struct in Rust that has info like `voice1_currentAmplitude` or `voice1_noteOn` events. The audio scheduling code updates these when notes play or with volume levels (maybe a simple low-pass filtered volume for smoothness).
  - Each frame, the render loop reads this info and applies to visuals. E.g., if `voice1_noteOn=true` at this frame, trigger an animation on voice1’s object (like scaling it up briefly). Or use `voice1_currentAmplitude` to set the scale continuously.
  - If using an `AnalyserNode`, we could pass an array of frequency magnitudes to Rust (via JS interop) and then use it in shader or CPU to animate an equalizer or color spectrum.

- **Shaders and Effects:**

  - Possibly write a shader that can be fed an intensity value to make objects glow or pulse. For instance, use a fragment shader that adds an emissive color proportional to the audio intensity.
  - Could use vertex shader to make the object scale or oscillate geometry slightly with audio.
  - Keep shaders simple enough for WebGPU – e.g., a basic Phong or PBR lighting if we have lights, or even unlit but colored shapes might suffice if artistic style is more abstract/neon.

- **Performance:**

  - WebGPU can handle many objects, but since our scene likely only has a handful of major objects (voices) plus some particles, it should be fine on modern GPUs. We will ensure to reuse GPU resources (don’t recreate buffers every frame unnecessarily, just update them).
  - Use instancing if we have many repeated elements (like a particle system or spectrum bars).
  - The visual updates should be synchronized to vsync (requestAnimationFrame locks to display refresh).
  - We should target 60 FPS for smooth visuals. If the scene grows in complexity, we can tune down effects or object count to maintain performance (especially because any stutter could also affect audio if on same thread).

- **Integration with UI:** Some visual objects might _be_ the UI controls. See next section on UI for specifics, but essentially, the visual scene will include not just purely decorative things but also interactive objects (like a button that is drawn as part of the 3D world). The visual engine will need to, for example, highlight an object when it’s hovered (if we can detect that) or animate it when clicked (to give feedback).

#### References

- [WebGPU API overview (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API)
- [wgpu crate docs](https://docs.rs/wgpu)
- [WGSL language spec](https://www.w3.org/TR/WGSL/)

### 3. User Interface & Interaction

**Philosophy:**
The UI is minimalist and embedded in the 3D world. The goal is that the user sees a beautiful scene that also _is_ the control surface. We avoid traditional HTML panels, buttons, sliders. Instead, the user interacts directly with visual elements to control the music. This requires careful design so that the controls are discoverable enough without labels.

**UI Controls (current implementation):**

- **Play/Pause:** Space key toggles pause/resume. No in-scene play/pause icon yet.
- **Regenerate:** `R` reseeds all voices. Per-voice: Shift+Click reseeds, Alt+Click solos, Click toggles mute.
- **Position Adjustment:** Click+drag on a voice's invisible interaction zone to move it on the horizontal plane; movement is clamped to a radius. Positions update the corresponding `PannerNode` in real time.
- **Tempo:** ArrowRight/ArrowLeft adjust BPM.
- **Overlay:** Start overlay for audio unlock; `H` toggles visibility. It does not show live BPM/Paused/Muted state.

**Possible UI Elements/Controls (future):**
We identify additional interactions that could be mapped to in-scene controls:

- **Play/Pause:** If the system allows stopping the music, a control to pause or resume generation. Perhaps the music runs by default and maybe we don’t need an explicit play (it starts immediately), but pause could be useful. Implement as an icon (e.g., a play/pause symbol) floating in a corner of the scene or as part of an object (maybe a central orb that stops/starts everything when clicked).
- **Regenerate (Randomize):** A control to generate a new musical sequence (either for all voices at once, or maybe separate control per voice). For all-at-once, an icon like 🔄 could be placed somewhere in view. For per-voice regeneration, perhaps clicking on a voice's invisible interaction zone could trigger it to come up with a new pattern.
- **Voice Mute/Unmute or Volume:** Perhaps clicking a voice object toggles it on/off (if user wants to focus on certain layers). If no labels, the object’s appearance can indicate mute state (e.g., dim or turn grey when muted). Volume could be controlled by distance: maybe the user drags the object closer or further from camera/listener to effectively change volume (since closer = louder in spatial audio). This would be a very natural metaphor for volume control!
- **Position Adjustment:** The user can **grab and move a voice’s object** in the 3D space. This changes the spatial position of that sound (panning/volume in headphones). It’s an interactive way for the user to do a sort of “mixing” – e.g., spread sounds out or bring one closer. We’ll implement drag controls:

  - On desktop, mouse click+drag on an object could move it. We need to implement a picking mechanism to select objects with the mouse. Possibly ray-cast from camera through cursor to find which object is clicked.
  - Simplify movement to perhaps a plane or spherical surface: e.g., restrict dragging to horizontal plane (x-z) so user won’t lose it in depth too much, or allow full 3D if we have a way to move in all axes (maybe using right-click or modifier for up/down).
  - As the voice position moves, update the corresponding PannerNode position in real-time so the sound appears from the new direction. This will likely impress the spatial effect on the user.

- **Change Scale/Key or Mode:** We might include a control for musical scale or mood. Perhaps a small set of preset scales (Major, Minor, Pentatonic, etc.) can be cycled. Without labels, this is tricky – maybe an object that cycles color and each color corresponds to a scale (could be hinted in some text in documentation or a minimal legend). Alternatively, the user might not need to change scale if the generative is fine by itself. This might be an advanced control possibly omitted in first version to keep UI simple.
- **Tempo Control:** If needed, could allow user to speed up or slow down. Perhaps a dial control represented by a ring around some object – the user dragging that ring could adjust tempo. Or simpler, two buttons (faster, slower) as plus/minus icons. But unlabeled plus/minus might be okay if intuitively placed next to a tempo icon (metronome icon?).
- **Visual Toggle:** Possibly a control to switch visualization modes or toggle particular visual elements. For example, toggling an “audio spectrum” display on/off. This could be a minor feature, added if time allows.

**UI Implementation Details:**

- **Event Handling:**

  - In the browser, capture mouse events on the canvas.
  - Perform **ray-sphere** intersection for voice picking. Maintain hover highlight; on click/drag, update engine voice state and audio panner.
  - Once we know which voice is selected on click, we handle according to that voice's role (e.g., if it's a voice: start dragging it; if it's a regenerate button: trigger regeneration immediately; etc.).
  - On drag: update voice position in real-time and possibly give some visual feedback through wave displacement effects.
  - On release: drop the voice at new position.
  - Also handle hover highlighting: as mouse moves, if it hovers an object, maybe slightly scale it up or change color to indicate it’s interactable. This can be done by checking ray intersection each frame with cursor position.

- **Integrated Look and Feel:**

- Keep controls integrated into the scene; avoid HTML-heavy UI. A minimalist hint overlay communicates keys and state (BPM/Paused/Muted).

- **Error Handling/State:** Ensure the UI accounts for states:

  - If a voice is muted/off, maybe its object appears “off” (dark or X over it).
  - If music is paused, maybe an overall tint changes or a big play icon appears to prompt resume.
  - If WebGPU or WebAudio initialization fails (browser not supported), provide a graceful message in HTML (since no heavy UI, maybe just an overlay). This is more about robust deployment than user feature, but worth noting to implement: check for WebGPU support via `navigator.gpu` existence and handle accordingly (like show "WebGPU not supported" message if not available).

### Cross-Platform Development Strategy

- **Testing:** Use `npm run check` to format, lint, test Rust, and build/serve the web bundle, then run the headless browser test (Puppeteer). CI skips engine-coupled assertions when WebGPU is unavailable in headless.
- **Platform Specific Limitations:**

  - Browser is single-threaded by default for WASM (unless using threading with Web Workers and shared memory, which is advanced). It may not be necessary to multi-thread this project heavily due to the scope (generating a few voices and moderate graphics can likely run on one thread). But if needed (for example heavy audio processing), consider using the web’s AudioWorklet (runs audio in a separate thread) or offload some calculations to a web worker.

- **Ignoring Mobile:** As stated, we will not optimize for mobile. If a user tries on mobile, one of two things likely happen: WebGPU not available (so it won’t run), or if it is (future), performance may be low. We can detect small screens and either warn or not officially support it. The UI also might not be touch-optimized yet (dragging with touch etc., which is additional complexity – not in scope now).

## Development Plan and Considerations

To ensure a "fantastic result", the development should proceed in stages, verifying each piece:

1. **Initial Setup:** Get a basic Rust+WASM project running with WebGPU rendering something simple (like a triangle or cube on screen) and Web Audio playing a test tone. This ensures the environment and build pipeline are correct (WebGPU initialization, etc.). Use this to verify browser compatibility (e.g., test in Chrome Canary or current stable with proper flags if needed).
2. **Basic 3D Scene (implemented):** The scene is in place with an ambient waves fullscreen pass that reacts to voice positions through displacement and proximity effects. There are no placeholder objects. The camera is fixed (the `AudioListener` tracks it for spatial audio). Interaction testing is via pointer hover/drag and keyboard; orbit/mouselook is not used.
3. **Audio Generation:** Implement the audio engine’s core:

   - Pick a scale (e.g., C major pentatonic) and generate a repeating random sequence for one voice. Use an OscillatorNode to play it. Ensure timing is consistent.
   - Expand to multiple voices. Start them together and verify the polyphonic mix sounds okay.
   - Add PannerNodes and separate positions for these voices. Put on headphones and verify spatial effect (voice sounds come from different directions).
   - Implement volume envelopes to avoid pops.

4. **Sync Audio-Visual:** Link the events. Have the visual objects respond to the audio – e.g., on each note event, flash or scale the corresponding object. Fine-tune to make it noticeable but not jarring.
5. **Interactivity:** Add the user interaction one by one:

   - Ray picking and dragging of voice positions. Ensure that moving a voice position changes its PannerNode coordinates and the wave displacement effects move accordingly.
   - Add a regenerate button or gesture. Perhaps a key press “R” for now to regenerate all sequences (for easier testing) – later replace with a 3D button.
   - Add a play/pause toggle (again, maybe key press first, then integrate UI object).
   - Test that these interactions can happen while audio is playing without glitching.

6. **UI Polish:** Create the actual 3D models or shapes for the controls decided (icons, etc.). Position them in scene (maybe slightly toward camera so they always are in view or even attach to camera view like HUD). Implement their interaction as done in test (just replacing the trigger from keypress to click on object).

   - Ensure they are not too obtrusive – perhaps semi-transparent or small until hovered.
   - No labels, so consider tooltips on hover (this might break the no-HTML rule, but maybe a tiny overlay canvas could show a word when hovering an icon). Alternatively, include a help modal accessible by a keyboard key.

7. **Visual Effects:** Enhance the visual responsiveness:

   - Possibly incorporate an FFT analysis to make some ambient visual element (like a waveform line or particles) react continuously to sound frequencies.
   - Add more dynamic lighting or postprocessing if desired (e.g., bloom effect for bright pulses, which can make the pulses from music more dramatic).
   - Ensure the color scheme is pleasing – perhaps assign each voice a base color and use those consistently (sound and visual correlation).
   - Make use of easing and animations so that changes are smooth (for example, when an amplitude goes up, lerp an object scale rather than instant jump, to make it look organic).

8. **Performance Tuning:** Profile the application:

   - Check that CPU usage in browser is reasonable and frames are not dropping. If audio scheduling is heavy, consider moving it to an AudioWorklet context.
   - Optimize any hotspots (for example, if we did per-frame JS<->Wasm data transfer for audio analysis, try to minimize data size or frequency).
   - Test on a variety of desktop hardware (including integrated GPUs, etc.) to ensure it runs at least at 60fps on typical systems.
   - Memory: ensure to drop or reuse WebAudio nodes to not leak memory (WebAudio can hold onto nodes if not properly disconnected).

<!-- Desktop version plan removed -->

10. **Refinement and UX:** Test the user experience thoroughly:

    - Is the generative music pleasant over long periods? Adjust algorithm parameters (note probabilities, etc.) to avoid annoying patterns or silence.
    - Is the spatial effect working well? Adjust positions or distance models (e.g., maybe use a mild distance attenuation so moving objects has a noticeable but not drastic volume change).
    - Are the controls discoverable? Perhaps conduct a user test where someone who hasn’t used it tries it – see if they understand how to interact. This might inform adding a minimal hint or tutorial at startup.
    - Visual appeal: Tweak colors, shapes, add any artistic assets needed so that the final result looks “fantastic”. We might incorporate simple textures or environment maps if it adds to scene (ensuring it doesn’t distract from main visualization).
    - Handle edge cases: e.g., if the user drags a sound object extremely far, do we limit the range? (We might clamp positions to some radius so they don’t throw it 100 meters away which could effectively mute it entirely or lose track visually).
    - Save settings: Not required, but could consider allowing the user to lock certain random seed or save a cool configuration. This may be beyond initial spec, but worth noting if future expansion is considered.

## Conclusion

This specification outlines a detailed plan to build an interactive 3D music visualizer with generative audio, using Rust, WebAssembly, and WebGPU. By focusing on a **browser-first implementation** and leveraging these modern technologies, we aim to achieve high-performance graphics and audio all within a web page. The system will offer users a unique blend of **algorithmic music creation and visual immersion**, all controlled through a sleek, subtle interface that feels like part of the art.

By following this spec, a developer should implement each component step by step – audio, visuals, and interaction – and ensure they seamlessly integrate. The end result will be a **novel creative application**: one where music generates itself under the hood, yet the user can shape and influence it in real time, both seeing and hearing the immediate impact of their actions. With Rust + WASM ensuring efficiency, and WebGPU enabling cutting-edge in-browser 3D rendering, this project will demonstrate a state-of-the-art web-based audio-visual experience.
