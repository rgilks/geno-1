use crate::core::MusicEngine;
use crate::core::{AEOLIAN, DORIAN, IONIAN, LOCRIAN, LYDIAN, MIXOLYDIAN, PHRYGIAN};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

#[inline]
pub fn root_midi_for_key(key: &str) -> Option<i32> {
    match key {
        "a" | "A" => Some(69), // A4
        "b" | "B" => Some(71), // B4
        "c" | "C" => Some(60), // C4 (middle C)
        "d" | "D" => Some(62), // D4
        "e" | "E" => Some(64), // E4
        "f" | "F" => Some(65), // F4
        "g" | "G" => Some(67), // G4
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
            let roots: [i32; 7] = [60, 62, 64, 65, 67, 69, 71]; // C, D, E, F, G, A, B
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
                        _ = doc.exit_fullscreen();
                    } else {
                        _ = canvas.request_fullscreen();
                    }
                }
            }
            ev.prevent_default();
        }
        "Escape" => {
            if let Some(win) = web::window() {
                if let Some(doc) = win.document() {
                    _ = doc.exit_fullscreen();
                }
            }
        }
        _ => {}
    }
    match key.as_str() {
        "ArrowUp" => {
            let v = master_gain.gain().value();
            let nv = (v + 0.05).min(1.0);
            _ = master_gain.gain().set_value(nv);
            ev.prevent_default();
        }
        "ArrowDown" => {
            let v = master_gain.gain().value();
            let nv = (v - 0.05).max(0.0);
            _ = master_gain.gain().set_value(nv);
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
        _ = window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
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
                super::keyboard::handle_global_keydown(
                    &ev,
                    &engine,
                    &paused,
                    &master_gain,
                    &canvas,
                );
            }) as Box<dyn FnMut(_)>);
        _ = window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}
