// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Local-only visibility policy.
//!
//! The desktop application never receives remote management policy, so all
//! local UI and tray actions remain available.

pub(crate) fn is_app_ui_hidden() -> bool {
    false
}

pub(crate) fn is_tray_item_hidden(_item: &str) -> bool {
    false
}

pub(crate) fn enterprise_json_hides_app_ui() -> bool {
    false
}
