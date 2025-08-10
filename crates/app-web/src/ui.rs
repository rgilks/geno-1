#![allow(clippy::unused_unit)]
use wasm_bindgen::JsCast;
use web_sys as web;

/// Toggle the visibility of the hint overlay.
pub fn toggle_hint_visibility(document: &web::Document) {
    if let Ok(Some(el)) = document.query_selector(".hint") {
        let was_visible = el
            .get_attribute("data-visible")
            .map(|v| v == "1")
            .unwrap_or(false);
        let show = !was_visible;
        let _ = el.set_attribute("data-visible", if show { "1" } else { "0" });
        let _ = el.set_attribute("style", if show { "" } else { "display:none" });
    }
}

/// Explicitly set the hint overlay visibility.
pub fn set_hint_visibility(document: &web::Document, visible: bool) {
    if let Ok(Some(el)) = document.query_selector(".hint") {
        let _ = el.set_attribute("data-visible", if visible { "1" } else { "0" });
        let _ = el.set_attribute("style", if visible { "" } else { "display:none" });
    }
}

/// Update the hint overlay content if it is currently visible.
/// This centralizes string formatting and DOM updates.
pub fn refresh_hint_if_visible(document: &web::Document, bpm: f32, paused: bool) {
    if let Ok(Some(el)) = document.query_selector(".hint") {
        if el.get_attribute("data-visible").as_deref() == Some("1") {
            if let Some(div) = el.dyn_ref::<web::HtmlElement>() {
                div.set_inner_html(&build_hint_content(bpm, paused));
            }
        }
    }
}

fn build_hint_content(bpm: f32, paused: bool) -> String {
    // Keep content concise and consistent across updates
    format!(
        "Click Start to begin<br />\
Click canvas: play a note • Mouse position affects sound<br />\
A..F: root • 1..7: mode • R: new seq • T: random key+mode • Space: pause/resume • ArrowLeft/Right: tempo • Enter: fullscreen<br />\
BPM: {:.0} • Paused: {}",
        bpm,
        if paused { "yes" } else { "no" }
    )
}
