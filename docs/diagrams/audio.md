## Audio Pipelines (Web and Native)

The diagrams below summarize how musical events are generated and rendered to audio on both web and native builds. They reflect the current implementation described in `docs/SPEC.md` and the code in `crates/app-core`, `crates/app-web`, and `crates/app-native`.

```mermaid
graph TD
  %% Audio pipelines (Web + Native)
  subgraph "Music Generation & Shared State"
    A["MusicEngine\n(eighth-note scheduler, BPM, scale)"]
    B["Shared Voice State\npositions • mute/solo"]
    A --> C["NoteEvent(s)\nvoice, freq, velocity, start, duration"]
    B -.-> D["Voice Positions\n(x,y,z)"]
  end

  %% Web Audio path
  subgraph "Audio (Web)"
    C --> W1["OscillatorNode\n(per note, per voice waveform)"]
    W1 --> W2["GainNode\n(attack/release envelope)"]
    W2 --> W3["PannerNode\n(HRTF/spatial panner)"]
    D -.-> W3
    W3 --> W4["Master Bus"]
    W3 -.-> WS1["Reverb Send"]
    WS1 --> WR["ConvolverNode\n(global reverb)"] --> W4
    W3 -.-> WS2["Delay Send"]
    WS2 --> WD["DelayNode\n(+ tone shaping)"] --> W4
    W4 --> W5["Master Gain / Mute"] --> W6["AnalyserNode (optional)"] --> W7["AudioContext.destination"]
  end

  %% Native audio path
  subgraph "Audio (Native)"
    C --> N1["ActiveOscillator\n(wave, amp, ADSR)"]
    D -.-> N2["Equal-power panning gains\nfrom X position"]
    N1 --> N3["mix_sample_stereo\n(sum all oscillators)"]
    N2 -.-> N3
    N3 --> N4["Master saturation (arctan)\nsubtle warmth/compression"] --> N5["CPAL output stream"]
  end
```

Notes:

- Web uses Web Audio nodes for envelope, spatialization, and buses (reverb, delay). An `AnalyserNode` is optional.
- Native uses `cpal` output; oscillators are mixed with equal-power stereo panning from voice X and subtle master saturation.
