#![allow(clippy::unused_unit)]
use wasm_bindgen::JsCast;
use web_sys as web;

pub fn toggle_hint_visibility(document: &web::Document) {
    if let Ok(Some(el)) = document.query_selector(".hint") {
        let cur = el.get_attribute("data-visible");
        let show = match cur.as_deref() {
            Some("1") => false,
            _ => true,
        };
        let _ = el.set_attribute("data-visible", if show { "1" } else { "0" });
        if let Some(div) = el.dyn_ref::<web::HtmlElement>() {
            if show {
                // Default content (before full engine/UI attach)
                div.set_inner_html(
                    "Click Start to begin • Drag to move a voice<br/>\
                     Click: mute • Shift+Click: reseed • Alt+Click: solo<br/>\
                     R: reseed all • Space: pause/resume • +/-: tempo • F: fullscreen<br/>\
                     BPM: 110 • Paused: no",
                );
                let _ = el.set_attribute("style", "");
            } else {
                let _ = el.set_attribute("style", "display:none");
            }
        }
    }
}
