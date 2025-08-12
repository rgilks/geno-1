#![cfg(target_arch = "wasm32")]
use crate::core::{EngineParams, MusicEngine, VoiceConfig, Waveform, C_MAJOR_PENTATONIC};
use glam::Vec3;
use instant::Instant;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys as web;
// (DeviceExt no longer needed; legacy vertex buffers removed)

mod audio;
mod camera;
mod constants;
mod core;
mod dom;
mod events;
mod frame;
mod input;
mod overlay;
mod render;
// ui module removed; overlay is controlled directly from here

// Rendering/picking shared constants live in `constants.rs`
fn wire_canvas_resize(canvas: &web::HtmlCanvasElement) {
    dom::sync_canvas_backing_size(canvas);
    let canvas_resize = canvas.clone();
    let resize_closure = Closure::wrap(Box::new(move || {
        dom::sync_canvas_backing_size(&canvas_resize);
    }) as Box<dyn FnMut()>);
    if let Some(window) = web::window() {
        _ = window
            .add_event_listener_with_callback("resize", resize_closure.as_ref().unchecked_ref());
    }
    resize_closure.forget();
}

struct InitParts {
    audio_ctx: web::AudioContext,
    listener_for_tick: web::AudioListener,
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
}

async fn build_audio_and_engine(_document: web::Document) -> anyhow::Result<InitParts> {
    let audio_ctx = web::AudioContext::new().map_err(|e| anyhow::anyhow!("{:?}", e))?;
    _ = audio_ctx.resume();
    let listener = audio_ctx.listener();
    listener.set_position(0.0, 0.0, 1.5);

    let voice_configs = vec![
        VoiceConfig {
            waveform: Waveform::Sine,
            base_position: Vec3::new(-1.0, 0.0, 0.0),
        },
        VoiceConfig {
            waveform: Waveform::Saw,
            base_position: Vec3::new(1.0, 0.0, 0.0),
        },
        VoiceConfig {
            waveform: Waveform::Triangle,
            base_position: Vec3::new(0.0, 0.0, -1.0),
        },
    ];
    let engine = Rc::new(RefCell::new(MusicEngine::new(
        voice_configs,
        EngineParams {
            bpm: 110.0,
            scale: C_MAJOR_PENTATONIC,
            root_midi: 60,
        },
        42,
    )));
    {
        let e = engine.borrow();
        log::info!(
            "[engine] voices={} pos0=({:.2},{:.2},{:.2}) pos1=({:.2},{:.2},{:.2}) pos2=({:.2},{:.2},{:.2})",
            e.voices.len(),
            e.voices[0].position.x, e.voices[0].position.y, e.voices[0].position.z,
            e.voices[1].position.x, e.voices[1].position.y, e.voices[1].position.z,
            e.voices[2].position.x, e.voices[2].position.y, e.voices[2].position.z
        );
    }
    let paused = Rc::new(RefCell::new(true));
    Ok(InitParts {
        audio_ctx,
        listener_for_tick: listener,
        engine,
        paused,
    })
}

fn wire_overlay_buttons(audio_ctx: &web::AudioContext, paused: &Rc<RefCell<bool>>) {
    if let Some(doc2) = dom::window_document() {
        let paused_ok = paused.clone();
        let audio_ok = audio_ctx.clone();
        dom::add_click_listener(&doc2, "overlay-ok", move || {
            *paused_ok.borrow_mut() = false;
            _ = audio_ok.resume();
            if let Some(w2) = web::window() {
                if let Some(d2) = w2.document() {
                    overlay::hide(&d2);
                }
            }
        });

        let paused_close = paused.clone();
        let audio_close = audio_ctx.clone();
        dom::add_click_listener(&doc2, "overlay-close", move || {
            *paused_close.borrow_mut() = false;
            _ = audio_close.resume();
            if let Some(w2) = web::window() {
                if let Some(d2) = w2.document() {
                    overlay::hide(&d2);
                }
            }
        });
    }
}
// noisy helper remnants removed

// analyser creation moved to audio::create_analyser

// global keydown moved to events.rs

// Create a GainNode with an initial value; logs on failure and returns None
// create_gain moved to audio.rs

// (use overlay::hide instead of local helper)

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

    // Note: start overlay is handled below (toggle with 'h') once audio is initialized.

    // Avoid grabbing a 2D context here to allow WebGPU to acquire the canvas

    // Maintain canvas internal pixel size to match CSS size * devicePixelRatio
    wire_canvas_resize(&canvas);

    // Prepare a clone for use inside the click closure
    let canvas_for_click = canvas.clone();

    // Start audio graph and scheduling + WebGPU renderer immediately; show overlay until OK/close
    static STARTED: AtomicBool = AtomicBool::new(false);
    {
        if STARTED.swap(true, Ordering::SeqCst) == false {
            let canvas_for_click_inner = canvas_for_click.clone();
            spawn_local(async move {
                let InitParts {
                    audio_ctx,
                    listener_for_tick,
                    engine,
                    paused,
                } = match build_audio_and_engine(document.clone()).await {
                    Ok(p) => p,
                    Err(_) => return,
                };

                wire_overlay_buttons(&audio_ctx, &paused);
                events::wire_overlay_toggle_h(&document);

                // FX buses
                let fx = match audio::build_fx_buses(&audio_ctx) {
                    Ok(f) => f,
                    Err(_) => return,
                };
                let master_gain = fx.master_gain.clone();
                let sat_pre = fx.sat_pre.clone();
                let sat_wet = fx.sat_wet.clone();
                let sat_dry = fx.sat_dry.clone();
                let reverb_in = fx.reverb_in.clone();
                let reverb_wet = fx.reverb_wet.clone();
                let delay_in = fx.delay_in.clone();
                let delay_feedback = fx.delay_feedback.clone();
                let delay_wet = fx.delay_wet.clone();

                // Per-voice master gains -> master bus, plus effect sends
                let initial_positions: Vec<Vec3> =
                    engine.borrow().voices.iter().map(|v| v.position).collect();
                let routing = match audio::wire_voices(
                    &audio_ctx,
                    &initial_positions,
                    &master_gain,
                    &delay_in,
                    &reverb_in,
                ) {
                    Ok(r) => r,
                    Err(_) => return,
                };
                let delay_sends = Rc::new(routing.delay_sends);
                let reverb_sends = Rc::new(routing.reverb_sends);
                let voice_panners = routing.voice_panners;
                let voice_gains = Rc::new(routing.voice_gains);

                // Initialize WebGPU
                let gpu: Option<render::GpuState> = frame::init_gpu(&canvas_for_click_inner).await;

                // Visual pulses per voice and optional analyser for ambient effects
                let pulses = Rc::new(RefCell::new(vec![0.0_f32; engine.borrow().voices.len()]));
                let (analyser, analyser_buf) = audio::create_analyser(&audio_ctx);

                // Queued ripple UV from pointer taps (read by render tick)
                let queued_ripple_uv: Rc<RefCell<Option<[f32; 2]>>> = Rc::new(RefCell::new(None));

                // ---------------- Interaction state ----------------
                let mouse_state = Rc::new(RefCell::new(input::MouseState::default()));
                let hover_index = Rc::new(RefCell::new(None::<usize>));
                let drag_state = Rc::new(RefCell::new(input::DragState::default()));

                // Keyboard controls
                events::wire_global_keydown(
                    engine.clone(),
                    paused.clone(),
                    master_gain.clone(),
                    canvas_for_click_inner.clone(),
                );

                // Pointer handlers (move/down/up)
                events::wire_input_handlers(events::InputWiring {
                    canvas: canvas_for_click_inner.clone(),
                    engine: engine.clone(),
                    mouse_state: mouse_state.clone(),
                    hover_index: hover_index.clone(),
                    drag_state: drag_state.clone(),
                    voice_gains: voice_gains.clone(),
                    delay_sends: delay_sends.clone(),
                    reverb_sends: reverb_sends.clone(),
                    audio_ctx: audio_ctx.clone(),
                    queued_ripple_uv: queued_ripple_uv.clone(),
                });

                // Scheduler + renderer loop driven by requestAnimationFrame
                let frame_ctx = Rc::new(RefCell::new(frame::FrameContext {
                    engine: engine.clone(),
                    paused: paused.clone(),
                    pulses: pulses.clone(),
                    hover_index: hover_index.clone(),
                    canvas: canvas_for_click_inner.clone(),
                    mouse: mouse_state.clone(),
                    audio_ctx: audio_ctx.clone(),
                    listener: listener_for_tick.clone(),
                    voice_gains: voice_gains.clone(),
                    delay_sends: delay_sends.clone(),
                    reverb_sends: reverb_sends.clone(),
                    voice_panners,
                    reverb_wet: reverb_wet.clone(),
                    delay_wet: delay_wet.clone(),
                    delay_feedback: delay_feedback.clone(),
                    sat_pre: sat_pre.clone(),
                    sat_wet: sat_wet.clone(),
                    sat_dry: sat_dry.clone(),
                    analyser: analyser.clone(),
                    analyser_buf: analyser_buf.clone(),
                    gpu,
                    queued_ripple_uv: queued_ripple_uv.clone(),
                    last_instant: Instant::now(),
                    prev_uv: [0.5, 0.5],
                    swirl_energy: 0.0,
                    swirl_pos: [0.5, 0.5],
                    swirl_vel: [0.0, 0.0],
                    swirl_initialized: false,
                    pulse_energy: [0.0, 0.0, 0.0],
                }));
                // Start RAF loop
                frame::start_loop(frame_ctx);
            });
        }
    }

    Ok(())
}

// (local GpuState definition removed; use `render::GpuState` exclusively)
