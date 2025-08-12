use crate::core::Waveform;
use glam::Vec3;
use std::cell::RefCell;
use std::rc::Rc;
use web_sys as web;

pub struct FxBuses {
    pub master_gain: web::GainNode,
    pub sat_pre: web::GainNode,
    pub sat_wet: web::GainNode,
    pub sat_dry: web::GainNode,
    pub reverb_in: web::GainNode,
    pub reverb_wet: web::GainNode,
    pub delay_in: web::GainNode,
    pub delay_feedback: web::GainNode,
    pub delay_wet: web::GainNode,
}

pub struct VoiceRouting {
    pub voice_gains: Vec<web::GainNode>,
    pub voice_panners: Vec<web::PannerNode>,
    pub delay_sends: Vec<web::GainNode>,
    pub reverb_sends: Vec<web::GainNode>,
}

fn create_gain(
    audio_ctx: &web::AudioContext,
    value: f32,
    label: &str,
) -> Result<web::GainNode, ()> {
    match web::GainNode::new(audio_ctx) {
        Ok(g) => {
            g.gain().set_value(value);
            Ok(g)
        }
        Err(e) => {
            log::error!("{} GainNode error: {:?}", label, e);
            Err(())
        }
    }
}

pub fn build_fx_buses(audio_ctx: &web::AudioContext) -> Result<FxBuses, ()> {
    // Master gain
    let master_gain = create_gain(audio_ctx, 0.25, "Master")?;

    // Subtle master saturation (arctan) with wet/dry mix
    let sat_pre = create_gain(audio_ctx, 0.9, "sat pre")?;
    #[allow(deprecated)]
    let saturator = web::WaveShaperNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("WaveShaperNode error: {:?}", e);
        })
        .map_err(|_| ())?;
    // Build arctan curve
    let curve_len: u32 = 2048;
    let drive: f32 = 1.6;
    let mut curve: Vec<f32> = Vec::with_capacity(curve_len as usize);
    for i in 0..curve_len {
        let x = (i as f32 / (curve_len - 1) as f32) * 2.0 - 1.0;
        curve.push((2.0 / std::f32::consts::PI) * (drive * x).atan());
    }
    #[allow(deprecated)]
    saturator.set_curve(Some(curve.as_mut_slice()));
    let sat_wet = create_gain(audio_ctx, 0.35, "sat wet")?;
    let sat_dry = create_gain(audio_ctx, 0.65, "sat dry")?;

    // Route master -> [dry,dst] and master -> pre -> shaper -> wet -> dst
    _ = master_gain.connect_with_audio_node(&sat_pre);
    _ = sat_pre.connect_with_audio_node(&saturator);
    _ = saturator.connect_with_audio_node(&sat_wet);
    _ = sat_wet.connect_with_audio_node(&audio_ctx.destination());
    _ = master_gain.connect_with_audio_node(&sat_dry);
    _ = sat_dry.connect_with_audio_node(&audio_ctx.destination());

    // Reverb bus
    let reverb_in = create_gain(audio_ctx, 1.0, "Reverb in")?;
    let reverb = web::ConvolverNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("ConvolverNode error: {:?}", e);
        })
        .map_err(|_| ())?;
    reverb.set_normalize(true);
    // Create a long, dark stereo impulse response procedurally
    {
        let sr = audio_ctx.sample_rate();
        let seconds = 5.0_f32; // lush tail
        let len = (sr as f32 * seconds) as u32;
        if let Ok(ir) = audio_ctx.create_buffer(2, len, sr) {
            // simple xorshift32 for deterministic noise
            let mut seed_l: u32 = 0x1234ABCD;
            let mut seed_r: u32 = 0x7890FEDC;
            for ch in 0..2 {
                let mut buf: Vec<f32> = vec![0.0; len as usize];
                let mut t = 0.0_f32;
                let dt = 1.0_f32 / sr as f32;
                for i in 0..len as usize {
                    let s = if ch == 0 { &mut seed_l } else { &mut seed_r };
                    let mut x = *s;
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    *s = x;
                    let n = ((x as f32 / std::u32::MAX as f32) * 2.0 - 1.0) as f32;
                    // Exponential decay envelope, dark tilt
                    let decay = (-t / 3.0).exp();
                    let dark = (1.0 - (t / seconds)).max(0.0);
                    let v = n * decay * (0.6 + 0.4 * dark);
                    buf[i] = v;
                    t += dt;
                }
                _ = ir.copy_to_channel(&mut buf, ch as i32);
            }
            reverb.set_buffer(Some(&ir));
        }
    }
    let reverb_wet = create_gain(audio_ctx, 0.6, "Reverb wet")?;
    _ = reverb_in.connect_with_audio_node(&reverb);
    _ = reverb.connect_with_audio_node(&reverb_wet);
    _ = reverb_wet.connect_with_audio_node(&master_gain);

    // Delay bus with feedback loop and lowpass tone for darkness
    let delay_in = create_gain(audio_ctx, 1.0, "Delay in")?;
    let delay = audio_ctx
        .create_delay_with_max_delay_time(3.0)
        .map_err(|e| {
            log::error!("DelayNode error: {:?}", e);
        })
        .map_err(|_| ())?;
    delay.delay_time().set_value(0.55);
    let delay_tone = web::BiquadFilterNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("BiquadFilterNode error: {:?}", e);
        })
        .map_err(|_| ())?;
    delay_tone.set_type(web::BiquadFilterType::Lowpass);
    delay_tone.frequency().set_value(1400.0);
    let delay_feedback = create_gain(audio_ctx, 0.6, "Delay feedback")?;
    let delay_wet = create_gain(audio_ctx, 0.5, "Delay wet")?;
    _ = delay_in.connect_with_audio_node(&delay);
    _ = delay.connect_with_audio_node(&delay_tone);
    _ = delay_tone.connect_with_audio_node(&delay_feedback);
    _ = delay_feedback.connect_with_audio_node(&delay);
    _ = delay_tone.connect_with_audio_node(&delay_wet);
    _ = delay_wet.connect_with_audio_node(&master_gain);

    Ok(FxBuses {
        master_gain,
        sat_pre,
        sat_wet,
        sat_dry,
        reverb_in,
        reverb_wet,
        delay_in,
        delay_feedback,
        delay_wet,
    })
}

// Fire a simple one-shot oscillator routed through a voice's gain and sends
pub fn trigger_one_shot(
    audio_ctx: &web::AudioContext,
    waveform: Waveform,
    frequency_hz: f32,
    velocity: f32,
    duration_sec: f64,
    voice_gain: &web::GainNode,
    delay_send: &web::GainNode,
    reverb_send: &web::GainNode,
) {
    if let Ok(src) = web::OscillatorNode::new(audio_ctx) {
        match waveform {
            Waveform::Sine => src.set_type(web::OscillatorType::Sine),
            // Waveform::Square => src.set_type(web::OscillatorType::Square),
            Waveform::Saw => src.set_type(web::OscillatorType::Sawtooth),
            Waveform::Triangle => src.set_type(web::OscillatorType::Triangle),
        }
        src.frequency().set_value(frequency_hz);
        if let Ok(g) = web::GainNode::new(audio_ctx) {
            g.gain().set_value(0.0);
            let now = audio_ctx.current_time();
            let t0 = now + 0.005;
            _ = g.gain().linear_ramp_to_value_at_time(velocity, t0 + 0.02);
            _ = g
                .gain()
                .linear_ramp_to_value_at_time(0.0, t0 + duration_sec);
            _ = src.connect_with_audio_node(&g);
            _ = g.connect_with_audio_node(voice_gain);
            _ = g.connect_with_audio_node(delay_send);
            _ = g.connect_with_audio_node(reverb_send);
            _ = src.start_with_when(t0);
            _ = src.stop_with_when(t0 + duration_sec + 0.05);
        }
    }
}

// Create analyser and an appropriately sized buffer
pub fn create_analyser(
    audio_ctx: &web::AudioContext,
) -> (Option<web::AnalyserNode>, Rc<RefCell<Vec<f32>>>) {
    let analyser: Option<web::AnalyserNode> = web::AnalyserNode::new(audio_ctx).ok();
    if let Some(a) = &analyser {
        a.set_fft_size(256);
    }
    let buf: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    if let Some(a) = &analyser {
        let bins = a.frequency_bin_count() as usize;
        buf.borrow_mut().resize(bins, 0.0);
    }
    (analyser, buf)
}

// Wire per-voice panners, gains and effect sends
pub fn wire_voices(
    audio_ctx: &web::AudioContext,
    initial_positions: &[Vec3],
    master_gain: &web::GainNode,
    delay_in: &web::GainNode,
    reverb_in: &web::GainNode,
) -> Result<VoiceRouting, ()> {
    let mut voice_gains: Vec<web::GainNode> = Vec::new();
    let mut voice_panners: Vec<web::PannerNode> = Vec::new();
    let mut delay_sends_vec: Vec<web::GainNode> = Vec::new();
    let mut reverb_sends_vec: Vec<web::GainNode> = Vec::new();

    for pos in initial_positions.iter() {
        let panner = web::PannerNode::new(audio_ctx)
            .map_err(|e| {
                log::error!("PannerNode error: {:?}", e);
            })
            .map_err(|_| ())?;
        panner.set_panning_model(web::PanningModelType::Hrtf);
        panner.set_distance_model(web::DistanceModelType::Inverse);
        panner.set_ref_distance(0.5);
        panner.set_max_distance(50.0);
        panner.position_x().set_value(pos.x as f32);
        panner.position_y().set_value(pos.y as f32);
        panner.position_z().set_value(pos.z as f32);

        let gain = create_gain(audio_ctx, 0.0, "Voice gain").map_err(|_| ())?;
        _ = gain.connect_with_audio_node(&panner);
        _ = panner.connect_with_audio_node(master_gain);

        let d_send = create_gain(audio_ctx, 0.4, "Delay send").map_err(|_| ())?;
        _ = d_send.connect_with_audio_node(delay_in);
        delay_sends_vec.push(d_send);

        let r_send = create_gain(audio_ctx, 0.65, "Reverb send").map_err(|_| ())?;
        _ = r_send.connect_with_audio_node(reverb_in);
        reverb_sends_vec.push(r_send);

        voice_gains.push(gain);
        voice_panners.push(panner);
    }

    Ok(VoiceRouting {
        voice_gains,
        voice_panners,
        delay_sends: delay_sends_vec,
        reverb_sends: reverb_sends_vec,
    })
}

// Public create_gain used across modules
// (no-op) use the Result-returning `create_gain` defined above for internal wiring
