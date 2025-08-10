use web_sys as web;

#[inline]
pub fn show(document: &web::Document) {
    if let Some(el) = document.get_element_by_id("start-overlay") {
        let _ = el.set_attribute("style", "");
    }
}

#[inline]
pub fn hide(document: &web::Document) {
    if let Some(el) = document.get_element_by_id("start-overlay") {
        let _ = el.set_attribute("style", "display:none");
    }
}

#[inline]
pub fn is_hidden(document: &web::Document) -> bool {
    document
        .get_element_by_id("start-overlay")
        .and_then(|el| el.get_attribute("style"))
        .map(|s| s.contains("display:none"))
        .unwrap_or(false)
}

#[inline]
pub fn toggle(document: &web::Document) {
    if is_hidden(document) {
        show(document);
    } else {
        hide(document);
    }
}


