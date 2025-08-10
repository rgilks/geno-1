use web_sys as web;

#[inline]
pub fn show(document: &web::Document) {
    if let Some(el) = document.get_element_by_id("start-overlay") {
        let cl = el.class_list();
        let _ = cl.remove_1("hidden");
        // fallback for environments without CSS class
        let _ = el.set_attribute("style", "");
    }
}

#[inline]
pub fn hide(document: &web::Document) {
    if let Some(el) = document.get_element_by_id("start-overlay") {
        let cl = el.class_list();
        let _ = cl.add_1("hidden");
        // fallback
        let _ = el.set_attribute("style", "display:none");
    }
}

#[inline]
pub fn is_hidden(document: &web::Document) -> bool {
    if let Some(el) = document.get_element_by_id("start-overlay") {
        if el.class_list().contains("hidden") { return true; }
        return el
            .get_attribute("style")
            .map(|s| s.contains("display:none"))
            .unwrap_or(false);
    }
    false
}

#[inline]
pub fn toggle(document: &web::Document) {
    if is_hidden(document) {
        show(document);
    } else {
        hide(document);
    }
}
