## Visual Pipeline

The diagram below summarizes the rendering flow in the app, including the ambient waves fullscreen pass and the post-processing stack.

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

Notes:

- `waves.wgsl` consumes voice positions and pulse to modulate displacement and highlights; pointer contributes swirl and click/tap ripple.
- `post.wgsl` implements bright-pass, separable blur and composite with ACES tonemap, vignette, grain.
- Instanced voice markers are not rendered by a separate pipeline currently; accents and highlights are integrated into `waves.wgsl` using per-voice uniforms.

References:

- [WebGPU API overview (MDN)](https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API)
- [wgpu crate docs](https://docs.rs/wgpu)
- [WGSL language spec](https://www.w3.org/TR/WGSL/)
