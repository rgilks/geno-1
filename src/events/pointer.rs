use crate::audio;
use crate::constants::CAMERA_Z;
use crate::core::{
    midi_to_hz, MusicEngine, ENGINE_DRAG_MAX_RADIUS, PICK_SPHERE_RADIUS, SPREAD, Z_OFFSET,
};
use crate::input;
use crate::render;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

pub struct InputWiring {
    pub canvas: web::HtmlCanvasElement,
    pub engine: Rc<RefCell<MusicEngine>>,
    pub mouse_state: Rc<RefCell<input::MouseState>>,
    pub hover_index: Rc<RefCell<Option<usize>>>,
    pub drag_state: Rc<RefCell<input::DragState>>,
    pub voice_gains: Rc<Vec<web::GainNode>>,
    pub delay_sends: Rc<Vec<web::GainNode>>,
    pub reverb_sends: Rc<Vec<web::GainNode>>,
    pub audio_ctx: web::AudioContext,
    pub queued_ripple_uv: Rc<RefCell<Option<[f32; 2]>>>,
}

pub fn wire_input_handlers(w: InputWiring) {
    // pointermove
    {
        let mouse_state_m = w.mouse_state.clone();
        let hover_m = w.hover_index.clone();
        let drag_m = w.drag_state.clone();
        let engine_m = w.engine.clone();
        let canvas_mouse = w.canvas.clone();
        let canvas_connected = canvas_mouse.is_connected();
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
            let pos = input::pointer_canvas_px(&ev, &canvas_mouse);
            if !canvas_connected {
                return;
            }
            {
                let mut ms = mouse_state_m.borrow_mut();
                ms.x = pos.x;
                ms.y = pos.y;
            }
            let (ro, rd) = render::screen_to_world_ray(&canvas_mouse, pos.x, pos.y, CAMERA_Z);
            let mut best = None::<(usize, f32)>;
            let z_offset = Z_OFFSET;
            for (i, v) in engine_m.borrow().voices.iter().enumerate() {
                let center_world = v.position * SPREAD + z_offset;
                if let Some(t) = input::ray_sphere(ro, rd, center_world, PICK_SPHERE_RADIUS) {
                    if t >= 0.0 {
                        match best {
                            Some((_, bt)) if t >= bt => {}
                            _ => best = Some((i, t)),
                        }
                    }
                }
            }
            if drag_m.borrow().active {
                let plane_z = drag_m.borrow().plane_z_world;
                if rd.z.abs() > 1e-6 {
                    let t = (plane_z - ro.z) / rd.z;
                    if t >= 0.0 {
                        let hit_world = ro + rd * t;
                        let mut eng_pos = (hit_world - Z_OFFSET) / SPREAD;
                        let max_r = ENGINE_DRAG_MAX_RADIUS;
                        let len = (eng_pos.x * eng_pos.x + eng_pos.z * eng_pos.z).sqrt();
                        if len > max_r {
                            let scale = max_r / len;
                            eng_pos.x *= scale;
                            eng_pos.z *= scale;
                        }
                        let mut eng = engine_m.borrow_mut();
                        let vi = drag_m.borrow().voice;
                        eng.set_voice_position(vi, glam::Vec3::new(eng_pos.x, 0.0, eng_pos.z));
                    }
                }
            } else {
                match best {
                    Some((i, _t)) => {
                        *hover_m.borrow_mut() = Some(i);
                    }
                    None => {
                        *hover_m.borrow_mut() = None;
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        if let Some(wnd) = web::window() {
            let _ = wnd
                .add_event_listener_with_callback("pointermove", closure.as_ref().unchecked_ref());
        }
        closure.forget();
    }

    // pointerdown
    {
        let hover_m = w.hover_index.clone();
        let drag_m = w.drag_state.clone();
        let mouse_m = w.mouse_state.clone();
        let engine_m = w.engine.clone();
        let canvas_target = w.canvas.clone();
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
            if let Some(i) = *hover_m.borrow() {
                let mut ds = drag_m.borrow_mut();
                ds.active = true;
                ds.voice = i;
                ds.plane_z_world = engine_m.borrow().voices[i].position.z * SPREAD + Z_OFFSET.z;
                log::info!("[mouse] begin drag on voice {}", i);
            }
            mouse_m.borrow_mut().down = true;
            let _ = canvas_target.set_pointer_capture(ev.pointer_id());
            ev.prevent_default();
        }) as Box<dyn FnMut(_)>);
        let _ = w
            .canvas
            .add_event_listener_with_callback("pointerdown", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    // pointerup
    {
        let hover_m = w.hover_index.clone();
        let drag_m = w.drag_state.clone();
        let mouse_m = w.mouse_state.clone();
        let engine_m = w.engine.clone();
        let voice_gains_click = w.voice_gains.clone();
        let delay_sends_click = w.delay_sends.clone();
        let reverb_sends_click = w.reverb_sends.clone();
        let canvas_click = w.canvas.clone();
        let audio_ctx_click = w.audio_ctx.clone();
        let ripple_queue = w.queued_ripple_uv.clone();
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
            let was_dragging = drag_m.borrow().active;
            if was_dragging {
                drag_m.borrow_mut().active = false;
            } else if let Some(i) = *hover_m.borrow() {
                let shift = ev.shift_key();
                let alt = ev.alt_key();
                if alt {
                    engine_m.borrow_mut().toggle_solo(i);
                } else if shift {
                    engine_m.borrow_mut().reseed_voice(i, None);
                } else {
                    engine_m.borrow_mut().toggle_mute(i);
                }
            } else {
                let [uvx, uvy] = input::pointer_canvas_uv(&ev, &canvas_click);
                if uvx.is_finite() && uvy.is_finite() {
                    let midi = 60.0 + uvx * 24.0;
                    let freq = midi_to_hz(midi as f32);
                    let vel = (0.35 + 0.65 * uvy) as f32;
                    let eng = engine_m.borrow();
                    let norm_xs: Vec<f32> = eng
                        .voices
                        .iter()
                        .map(|v| (v.position.x / 3.0).clamp(-1.0, 1.0) * 0.5 + 0.5)
                        .collect();
                    let best_i = crate::input::nearest_index_by_uvx(&norm_xs, uvx);
                    let dur = 0.35 + 0.25 * (1.0 - uvy as f64);
                    let wf = eng.configs[best_i].waveform;
                    drop(eng);
                    audio::trigger_one_shot(
                        &audio_ctx_click,
                        wf,
                        freq,
                        vel,
                        dur,
                        &voice_gains_click[best_i],
                        &delay_sends_click[best_i],
                        &reverb_sends_click[best_i],
                    );
                    *ripple_queue.borrow_mut() = Some([uvx, uvy]);
                }
            }
            mouse_m.borrow_mut().down = false;
            ev.prevent_default();
        }) as Box<dyn FnMut(_)>);
        if let Some(wnd) = web::window() {
            let _ =
                wnd.add_event_listener_with_callback("pointerup", closure.as_ref().unchecked_ref());
        }
        closure.forget();
    }
}
