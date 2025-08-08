use app_core::{EngineParams, MusicEngine, VoiceConfig, Waveform, C_MAJOR_PENTATONIC};
use glam::Vec3;
use instant::Instant;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys as web;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();
    log::info!("app-web starting");

    spawn_local(async move {
        if let Err(e) = init().await {
            log::error!("init error: {:?}", e);
        }
    });
    Ok(())
}

async fn init() -> anyhow::Result<()> {
    let window = web::window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    let document = window
        .document()
        .ok_or_else(|| anyhow::anyhow!("no document"))?;

    let canvas_el = document
        .get_element_by_id("app-canvas")
        .ok_or_else(|| anyhow::anyhow!("missing #app-canvas"))?;
    let canvas: web::HtmlCanvasElement = canvas_el
        .dyn_into::<web::HtmlCanvasElement>()
        .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;

    // Minimal visual feedback background
    if let Ok(ctx2d) = canvas.get_context("2d") {
        if let Some(ctx) = ctx2d {
            let ctx: web::CanvasRenderingContext2d = ctx.dyn_into().unwrap();
            ctx.set_fill_style(&JsValue::from_str("#04060a"));
            ctx.fill_rect(0.0, 0.0, canvas.width() as f64, canvas.height() as f64);
        }
    }

    // Prepare a clone for use inside the click closure
    let canvas_for_click = canvas.clone();

    // On first click, start audio graph and scheduling
    {
        let closure = Closure::wrap(Box::new(move || {
            // Build AudioContext in response to user gesture
            let audio_ctx = match web::AudioContext::new() {
                Ok(ctx) => ctx,
                Err(e) => {
                    log::error!("AudioContext error: {:?}", e);
                    return;
                }
            };
            let listener = audio_ctx.listener();
            listener.set_position(0.0, 0.0, 1.5);

            // Music engine
            let voice_configs = vec![
                VoiceConfig {
                    color_rgb: [0.9, 0.3, 0.3],
                    waveform: Waveform::Sine,
                    base_position: Vec3::new(-1.0, 0.0, 0.0),
                },
                VoiceConfig {
                    color_rgb: [0.3, 0.9, 0.4],
                    waveform: Waveform::Saw,
                    base_position: Vec3::new(1.0, 0.0, 0.0),
                },
                VoiceConfig {
                    color_rgb: [0.3, 0.5, 0.9],
                    waveform: Waveform::Triangle,
                    base_position: Vec3::new(0.0, 0.0, -1.0),
                },
            ];
            let mut engine = MusicEngine::new(
                voice_configs,
                EngineParams {
                    bpm: 110.0,
                    scale: C_MAJOR_PENTATONIC,
                },
                42,
            );

            // Per-voice master gains -> destination
            let mut voice_gains: Vec<web::GainNode> = Vec::new();
            for _ in 0..engine.voices.len() {
                let gain = match web::GainNode::new(&audio_ctx) {
                    Ok(g) => g,
                    Err(e) => {
                        log::error!("GainNode error: {:?}", e);
                        return;
                    }
                };
                gain.gain().set_value(0.2);
                if let Err(e) = gain.connect_with_audio_node(&audio_ctx.destination()) {
                    log::error!("connect error: {:?}", e);
                    return;
                }
                voice_gains.push(gain);
            }

            // Scheduler loop
            let mut last_instant = Instant::now();
            let mut note_events = Vec::new();
            let canvas_clone = canvas_for_click.clone();
            let tick = Closure::wrap(Box::new(move || {
                let now = Instant::now();
                let dt = now - last_instant;
                last_instant = now;
                let audio_time = audio_ctx.current_time();
                note_events.clear();
                engine.tick(dt, audio_time, &mut note_events);

                for ev in &note_events {
                    let src = match web::OscillatorNode::new(&audio_ctx) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    match engine.configs[ev.voice_index].waveform {
                        Waveform::Sine => src.set_type(web::OscillatorType::Sine),
                        Waveform::Square => src.set_type(web::OscillatorType::Square),
                        Waveform::Saw => src.set_type(web::OscillatorType::Sawtooth),
                        Waveform::Triangle => src.set_type(web::OscillatorType::Triangle),
                    }
                    src.frequency().set_value(ev.frequency_hz);

                    let gain = match web::GainNode::new(&audio_ctx) {
                        Ok(g) => g,
                        Err(_) => continue,
                    };
                    gain.gain().set_value(0.0);
                    let t0 = audio_time + 0.01;
                    let _ = gain
                        .gain()
                        .linear_ramp_to_value_at_time(ev.velocity as f32, t0 + 0.02);
                    let _ = gain
                        .gain()
                        .linear_ramp_to_value_at_time(0.0_f32, t0 + ev.duration_sec as f64);

                    let _ = src.connect_with_audio_node(&gain);
                    let _ = gain.connect_with_audio_node(&voice_gains[ev.voice_index]);

                    let _ = src.start_with_when(t0);
                    let _ = src.stop_with_when(t0 + ev.duration_sec as f64 + 0.02);
                }

                if let Ok(ctx2d) = canvas_clone.get_context("2d") {
                    if let Some(ctx) = ctx2d {
                        let ctx: web::CanvasRenderingContext2d = ctx.dyn_into().unwrap();
                        ctx.set_fill_style(&JsValue::from_str("#050a12"));
                        ctx.fill_rect(
                            0.0,
                            0.0,
                            canvas_clone.width() as f64,
                            canvas_clone.height() as f64,
                        );
                    }
                }
            }) as Box<dyn FnMut()>);

            let _ = web::window()
                .unwrap()
                .set_interval_with_callback_and_timeout_and_arguments_0(
                    tick.as_ref().unchecked_ref(),
                    16,
                );
            tick.forget();
        }) as Box<dyn FnMut()>);
        canvas
            .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
            .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;
        closure.forget();
    }

    Ok(())
}
