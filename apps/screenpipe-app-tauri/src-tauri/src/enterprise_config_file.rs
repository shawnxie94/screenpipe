// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Compatibility helper for removing legacy enterprise settings.
//!
//! It does not contact a service. New local-only code must not use it.

use serde_json::{Map, Value};
use std::path::Path;

pub(crate) fn update(
    path: &Path,
    mutate: impl FnOnce(&mut Map<String, Value>) -> Result<(), String>,
) -> Result<bool, String> {
    let mut object = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    mutate(&mut object)?;
    if object.is_empty() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&object).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    Ok(true)
}
