use js_sys::{Array, Reflect};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    DedicatedWorkerGlobalScope, ErrorEvent, EventTarget, MessageEvent, Worker, WorkerOptions,
    WorkerType,
};

/// Own both the registration and the callback; no permanent forgotten closures.
pub(super) struct Listener {
    target: EventTarget,
    event: &'static str,
    callback: Closure<dyn FnMut(web_sys::Event)>,
}
impl Listener {
    pub fn new(
        target: EventTarget,
        event: &'static str,
        callback: impl FnMut(web_sys::Event) + 'static,
    ) -> Result<Self, JsValue> {
        let callback = Closure::new(callback);
        target.add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())?;
        Ok(Self {
            target,
            event,
            callback,
        })
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback(
            self.event,
            self.callback.as_ref().unchecked_ref(),
        );
    }
}
pub(super) struct Timer {
    id: i32,
    _callback: Closure<dyn FnMut()>,
}
impl Timer {
    pub fn new(milliseconds: f64, callback: impl Fn() + 'static) -> Result<Self, JsValue> {
        let callback = Closure::new(callback);
        let id = web_sys::window()
            .ok_or("window unavailable")?
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                milliseconds.clamp(0., i32::MAX as f64) as i32,
            )?;
        Ok(Self {
            id,
            _callback: callback,
        })
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_timeout_with_handle(self.id);
        }
    }
}

pub(super) struct WorkerHandle {
    pub worker: Worker,
    _listeners: Vec<Listener>,
}
impl WorkerHandle {
    pub fn new(
        message: impl Fn(JsValue) + 'static,
        error: impl Fn(JsValue) + 'static,
    ) -> Result<Self, JsValue> {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options("./worker.js", &options)?;
        let listeners = vec![
            Listener::new(worker.clone().into(), "message", move |event| {
                message(event.unchecked_into::<MessageEvent>().data())
            })?,
            Listener::new(worker.clone().into(), "error", move |event| {
                let event = event.unchecked_into::<ErrorEvent>();
                event.prevent_default();
                error(event.message().into());
            })?,
        ];
        Ok(Self {
            worker,
            _listeners: listeners,
        })
    }
}
impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.worker.terminate();
    }
}

thread_local! { static WORKER_LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) }; }
/// Register the worker's message protocol in Rust after the bootstrap instantiates WASM.
#[wasm_bindgen]
pub fn worker_main() -> Result<(), JsValue> {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().dyn_into()?;
    let sender = scope.clone();
    let listener = Listener::new(scope.clone().into(), "message", move |event| {
        let config = event.unchecked_into::<MessageEvent>().data();
        let result = super::config::solve(&config).unwrap_or_else(|error| {
            let result = Array::new();
            result.push(&"error".into());
            result.push(&error);
            result
        });
        let packet = Array::new();
        packet.push(&Reflect::get(&config, &"ticket".into()).unwrap_or(JsValue::UNDEFINED));
        packet.push(&result);
        let _ = sender.post_message(&packet);
    })?;
    WORKER_LISTENER.with(|slot| *slot.borrow_mut() = Some(listener));
    scope.post_message(&"ready".into())
}

pub(super) fn set(config: &JsValue, key: &str, value: impl Into<JsValue>) -> Result<(), JsValue> {
    Reflect::set(config, &key.into(), &value.into()).map(|_| ())
}
pub(super) fn copy_config(config: &JsValue) -> Result<JsValue, JsValue> {
    let copy = js_sys::Object::new();
    for key in Reflect::own_keys(config)? {
        Reflect::set(&copy, &key, &Reflect::get(config, &key)?)?;
    }
    Ok(copy.into())
}
/// Defer state changes until after the current JS callback has returned, so
/// terminating a worker can safely release that callback's Rust closure.
pub(super) fn defer(callback: impl FnOnce() + 'static) {
    spawn_local(async move {
        callback();
    });
}
