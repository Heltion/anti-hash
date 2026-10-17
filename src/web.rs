//! Browser behavior and worker orchestration are implemented in Rust.
mod config;
mod runtime;
mod session;
mod ui;

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    if web_sys::window().is_some() {
        ui::start()?;
    }
    Ok(())
}
