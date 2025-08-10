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
                let _ = el.set_attribute("style", "");
            } else {
                let _ = el.set_attribute("style", "display:none");
            }
        }
    }
}
