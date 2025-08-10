use crate::audio;
use crate::input;
use crate::render;
use app_core::MusicEngine;
use app_core::{
    midi_to_hz, z_offset_vec3, AEOLIAN, DORIAN, ENGINE_DRAG_MAX_RADIUS, IONIAN, LOCRIAN, LYDIAN,
    MIXOLYDIAN, PHRYGIAN, PICK_SPHERE_RADIUS, SPREAD,
};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

#[inline]
pub fn root_midi_for_key(key: &str) -> Option<i32> {
    match key {
        "a" | "A" => Some(69),
        "b" | "B" => Some(71),
        "c" | "C" => Some(60),
        "d" | "D" => Some(62),
        "e" | "E" => Some(64),
        "f" | "F" => Some(65),
        _ => None,
    }
}

#[inline]
pub fn mode_scale_for_digit(key: &str) -> Option<&'static [i32]> {
    match key {
        "1" => Some(IONIAN),
        "2" => Some(DORIAN),
        "3" => Some(PHRYGIAN),
        "4" => Some(LYDIAN),
        "5" => Some(MIXOLYDIAN),
        "6" => Some(AEOLIAN),
        "7" => Some(LOCRIAN),
        _ => None,
    }
}

pub fn handle_global_keydown(
    ev: &web::KeyboardEvent,
    engine: &Rc<RefCell<MusicEngine>>,
    paused: &Rc<RefCell<bool>>,
    master_gain: &web::GainNode,
    canvas: &web::HtmlCanvasElement,
) {
    let key = ev.key();
    if let Some(midi) = root_midi_for_key(&key) {
        engine.borrow_mut().params.root_midi = midi;
        return;
    }
    if let Some(scale) = mode_scale_for_digit(&key) {
        engine.borrow_mut().params.scale = scale;
        return;
    }
    match key.as_str() {
        "r" | "R" => {
            let voice_len = engine.borrow().voices.len();
            let mut eng = engine.borrow_mut();
            for i in 0..voice_len {
                eng.reseed_voice(i, None);
            }
        }
        "t" | "T" => {
            let roots: [i32; 7] = [60, 62, 64, 65, 67, 69, 71];
            let modes: [&'static [i32]; 7] = [
                IONIAN, DORIAN, PHRYGIAN, LYDIAN, MIXOLYDIAN, AEOLIAN, LOCRIAN,
            ];
            let ri = (js_sys::Math::random() * roots.len() as f64).floor() as usize;
            let mi = (js_sys::Math::random() * modes.len() as f64).floor() as usize;
            let mut eng = engine.borrow_mut();
            eng.params.root_midi = roots[ri];
            eng.params.scale = modes[mi];
        }
        " " => {
            let mut p = paused.borrow_mut();
            *p = !*p;
            ev.prevent_default();
        }
        "ArrowRight" | "+" | "=" => {
            let mut eng = engine.borrow_mut();
            let new_bpm = (eng.params.bpm + 5.0).min(240.0);
            eng.set_bpm(new_bpm);
        }
        "ArrowLeft" | "-" | "_" => {
            let mut eng = engine.borrow_mut();
            let new_bpm = (eng.params.bpm - 5.0).max(40.0);
            eng.set_bpm(new_bpm);
        }
        "Enter" => {
            if let Some(win) = web::window() {
                if let Some(doc) = win.document() {
                    if doc.fullscreen_element().is_some() {
                        let _ = doc.exit_fullscreen();
                    } else {
                        let _ = canvas.request_fullscreen();
                    }
                }
            }
            ev.prevent_default();
        }
        "Escape" => {
            if let Some(win) = web::window() {
                if let Some(doc) = win.document() {
                    let _ = doc.exit_fullscreen();
                }
            }
        }
        _ => {}
    }
    match key.as_str() {
        "ArrowUp" => {
            let v = master_gain.gain().value();
            let nv = (v + 0.05).min(1.0);
            let _ = master_gain.gain().set_value(nv);
            ev.prevent_default();
        }
        "ArrowDown" => {
            let v = master_gain.gain().value();
            let nv = (v - 0.05).max(0.0);
            let _ = master_gain.gain().set_value(nv);
            ev.prevent_default();
        }
        _ => {}
    }
}

// Wire an 'H' key handler to toggle the overlay without affecting pause state
pub fn wire_overlay_toggle_h(document: &web::Document) {
    if let Some(window) = web::window() {
        let doc = document.clone();
        let closure =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::KeyboardEvent| {
                let key = ev.key();
                if key == "h" || key == "H" {
                    crate::overlay::toggle(&doc);
                    ev.prevent_default();
                }
            }) as Box<dyn FnMut(_)>);
        let _ =
            window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

pub fn wire_global_keydown(
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
    master_gain: web::GainNode,
    canvas: web::HtmlCanvasElement,
) {
    if let Some(window) = web::window() {
        let closure =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::KeyboardEvent| {
                super::events::handle_global_keydown(&ev, &engine, &paused, &master_gain, &canvas);
            }) as Box<dyn FnMut(_)>);
        let _ =
            window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

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
            let (ro, rd) =
                render::screen_to_world_ray(&canvas_mouse, pos.x, pos.y, super::CAMERA_Z);
            let mut best = None::<(usize, f32)>;
            let z_offset = z_offset_vec3();
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
                        let mut eng_pos = (hit_world - z_offset_vec3()) / SPREAD;
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
                ds.plane_z_world =
                    engine_m.borrow().voices[i].position.z * SPREAD + z_offset_vec3().z;
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
