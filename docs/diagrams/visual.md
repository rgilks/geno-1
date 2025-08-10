## Visual Pipeline (Web)

The diagram below summarizes the rendering flow for the web build, including the ambient waves background and the post-processing stack.

```mermaid
graph TD
  %% Visual pipeline (Web)
  subgraph "Input & Audio Reactivity"
    I1["Pointer / Keyboard events"]
    I2["NoteEvent Pulses\n(per voice velocity → pulse)"]
    I3["Shared Voice State\npositions • colors • pulses"]
    I1 -->|drag, click, keys| I3
    I2 -->|on note| I3
  end

  subgraph "Scene Rendering"
    R0["WGPU Device + Surface"]
    R1["Scene Pipeline (scene.wgsl)\nInstanced voice markers (quads→disks)\nInputs: positions, colors, pulse"]
    R2["Ambient Waves (waves.wgsl)\nFullscreen pass; swirl, voice displacement, ripple"]
    R3["Post Stack (post.wgsl)\nBright pass → Separable blur → Composite\nACES tonemap, vignette, grain"]
    R4["Swapchain Present"]
    I3 -.->|per frame| R1
    I3 -.->|uniforms| R2
    R1 --> R3
    R2 --> R3
    R3 --> R4
  end

  subgraph "Web specifics"
    WGPU["WebGPU on Canvas"]
    AC["AudioListener tied to camera"]
    R0 --> WGPU
    I3 -.-> AC
  end
```

Notes:

- `waves.wgsl` consumes voice positions and pulse to modulate displacement and highlights; pointer contributes swirl and click/tap ripple.
- `post.wgsl` implements bright-pass, separable blur and composite with ACES tonemap, vignette, grain.
- Instanced voice markers are rendered with `scene.wgsl`, using per-instance position, color, and pulse.
