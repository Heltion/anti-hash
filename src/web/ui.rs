use super::{
    runtime::{Listener, Timer, defer, set},
    session::Session,
};
use crate::auto_length::WorkerSettings;
use js_sys::{Object, Reflect};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Document, Element, Event, HtmlButtonElement, HtmlInputElement, HtmlSelectElement};

pub(super) const DIRECTIONS: [&str; 2] = ["h ≡ ∑ si q^i (mod p)", "h ≡ ∑ si q^(n-1-i) (mod p)"];
const MODULI: [&str; 4] = [
    "998244353",
    "1000000007",
    "1000000000000000003",
    "18446744073709551616",
];
const BASES: [&str; 4] = ["233", "27", "257", "114514"];

pub(super) struct App {
    pub document: Document,
    pub settings: WorkerSettings,
    pub supported: bool,
    pub running: RefCell<Option<Session>>,
    pub sequence: Cell<u64>,
    rows: Cell<usize>,
    listeners: RefCell<Vec<Listener>>,
    copy_timers: RefCell<BTreeMap<&'static str, Timer>>,
}
impl App {
    pub fn element(&self, id: &str) -> Element {
        self.document
            .get_element_by_id(id)
            .expect("application markup")
    }
    pub fn input(&self, id: &str) -> HtmlInputElement {
        self.element(id).unchecked_into()
    }
    pub fn select(&self, id: &str) -> HtmlSelectElement {
        self.element(id).unchecked_into()
    }
    pub fn text(&self, id: &str, text: &str) {
        self.element(id).set_text_content(Some(text));
    }
    pub fn hidden(&self, id: &str, hidden: bool) {
        self.element(id)
            .toggle_attribute_with_force("hidden", hidden)
            .expect("DOM attribute");
    }
    pub fn error(&self, error: JsValue) {
        self.text(
            "log",
            &error.as_string().unwrap_or_else(|| format!("{error:?}")),
        );
    }
    fn listen(
        self: &Rc<Self>,
        id: &str,
        event: &'static str,
        callback: impl Fn(Rc<Self>, Event) -> Result<(), JsValue> + 'static,
    ) -> Result<(), JsValue> {
        let weak = Rc::downgrade(self);
        let listener = Listener::new(self.element(id).into(), event, move |event| {
            if let Some(app) = weak.upgrade()
                && let Err(error) = callback(Rc::clone(&app), event)
            {
                app.error(error);
            }
        })?;
        self.listeners.borrow_mut().push(listener);
        Ok(())
    }
    pub fn snapshot(&self) -> Result<JsValue, JsValue> {
        let config = Object::new();
        let fields = self
            .document
            .query_selector_all("#search input, #search select")?;
        for i in 0..fields.length() {
            let node = fields.item(i).expect("form field");
            let (id, value) = if let Some(input) = node.dyn_ref::<HtmlInputElement>() {
                (
                    input.id(),
                    if input.type_() == "checkbox" {
                        input.checked().to_string()
                    } else {
                        input.value().trim().to_owned()
                    },
                )
            } else if let Some(select) = node.dyn_ref::<HtmlSelectElement>() {
                (select.id(), select.value())
            } else {
                continue;
            };
            set(&config, &id, value)?;
        }
        Ok(config.into())
    }
    pub fn busy(&self, busy: bool) {
        let fields = self
            .document
            .query_selector_all("#search input, #search select")
            .expect("form fields");
        for i in 0..fields.length() {
            let node = fields.item(i).expect("form field");
            if let Some(input) = node.dyn_ref::<HtmlInputElement>() {
                input.set_disabled(busy);
            }
            if let Some(select) = node.dyn_ref::<HtmlSelectElement>() {
                select.set_disabled(busy);
            }
        }
        self.element("generate")
            .unchecked_into::<HtmlButtonElement>()
            .set_disabled(busy);
        self.hidden("stop", !busy);
    }
    fn update_rows(&self) -> Result<(), JsValue> {
        let Ok(count) = self.input("number").value().parse::<usize>() else {
            return Ok(());
        };
        if !(1..=100).contains(&count) {
            return Ok(());
        }
        while self.rows.get() < count {
            let i = self.rows.get();
            let row = self.document.create_element("tr")?;
            row.set_id(&format!("input_{i}"));
            let index = self.document.create_element("th")?;
            index.set_attribute("scope", "row")?;
            index.set_text_content(Some(&(i + 1).to_string()));
            row.append_child(&index)?;
            for (kind, value) in [
                ("modulo", MODULI.get(i).copied().unwrap_or("1")),
                ("base", BASES.get(i).copied().unwrap_or("0")),
            ] {
                let cell = self.document.create_element("td")?;
                let input: HtmlInputElement = self.document.create_element("input")?.dyn_into()?;
                input.set_id(&format!("{kind}_{i}"));
                input.set_attribute("inputmode", "numeric")?;
                input.set_value(value);
                input.set_attribute(
                    "aria-label",
                    &format!(
                        "{}, pair {}",
                        if kind == "modulo" { "Modulo" } else { "Base" },
                        i + 1
                    ),
                )?;
                cell.append_child(&input)?;
                row.append_child(&cell)?;
            }
            self.element("hashes").append_child(&row)?;
            self.rows.set(i + 1);
        }
        for i in 0..self.rows.get() {
            self.hidden(&format!("input_{i}"), i >= count);
        }
        Ok(())
    }
    fn update_alphabet(&self) {
        let (maximum, hint) = match self.select("alphabet").value().as_str() {
            "digits" => (10, "2–10 digits, starting at 0."),
            "both" => (36, "2–36 characters: 0–9, then a–z."),
            _ => (26, "2–26 lowercase letters, starting at a."),
        };
        self.input("sigma").set_max(&maximum.to_string());
        self.input("sigma").set_value(&maximum.to_string());
        self.text("alphabet_hint", hint);
    }
    fn setup_workers(&self, cpus: Option<usize>) -> Result<(), JsValue> {
        let select = self.select("workers");
        select.set_text_content(None);
        let option = |value: &str, label: String| -> Result<(), JsValue> {
            let option = self.document.create_element("option")?;
            option.set_attribute("value", value)?;
            option.set_text_content(Some(&label));
            select.append_child(&option)?;
            Ok(())
        };
        if let Some(count) = self.settings.automatic {
            option("auto", format!("Auto ({count} workers)"))?;
        }
        for count in 1..=self.settings.maximum {
            option(&count.to_string(), count.to_string())?;
        }
        option("0", "Off (main thread)".into())?;
        select.set_value(if self.settings.automatic.is_some() {
            "auto"
        } else {
            "1"
        });
        let hint = if !self.supported {
            "Web Workers unavailable · searches run sequentially.".into()
        } else if let Some(cpus) = cpus {
            format!(
                "{cpus} logical CPUs detected · Auto uses up to 4; choose 1–{} manually.",
                self.settings.maximum
            )
        } else {
            "CPU count unavailable · default is 1 worker.".into()
        };
        self.text("worker_hint", &hint);
        Ok(())
    }
    fn reverse(&self) {
        let reversed = self
            .element("reverse_description")
            .text_content()
            .as_deref()
            == Some(DIRECTIONS[1]);
        self.text("reverse_description", DIRECTIONS[usize::from(!reversed)]);
        for id in ["string_a", "string_b"] {
            self.text(
                id,
                &self
                    .element(id)
                    .text_content()
                    .unwrap_or_default()
                    .chars()
                    .rev()
                    .collect::<String>(),
            );
        }
    }
    fn copy(self: &Rc<Self>, name: &'static str) {
        self.copy_timers.borrow_mut().remove(name);
        let button: HtmlButtonElement = self.element(&format!("copy_{name}")).unchecked_into();
        button.set_disabled(true);
        let value = self
            .element(&format!("string_{name}"))
            .text_content()
            .unwrap_or_default();
        let promise = web_sys::window()
            .expect("window")
            .navigator()
            .clipboard()
            .write_text(&value);
        let app = Rc::clone(self);
        spawn_local(async move {
            let success = JsFuture::from(promise).await.is_ok();
            button.set_disabled(false);
            button.set_text_content(Some(if success { "Copied ✓" } else { "Copy failed" }));
            let _ = button.set_attribute(
                "aria-label",
                &format!(
                    "{} string {}",
                    if success { "Copied" } else { "Copy failed for" },
                    name.to_uppercase()
                ),
            );
            let _ = button.toggle_attribute_with_force("data-copied", success);
            let weak = Rc::downgrade(&app);
            if let Ok(timer) = Timer::new(2000., move || {
                let weak = weak.clone();
                defer(move || {
                    if let Some(app) = weak.upgrade() {
                        let button = app.element(&format!("copy_{name}"));
                        button.set_text_content(Some("Copy"));
                        let _ = button.set_attribute(
                            "aria-label",
                            &format!("Copy string {}", name.to_uppercase()),
                        );
                        let _ = button.remove_attribute("data-copied");
                        app.copy_timers.borrow_mut().remove(name);
                    }
                });
            }) {
                app.copy_timers.borrow_mut().insert(name, timer);
            }
        });
    }
}
thread_local! { static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) }; }
pub(super) fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("window unavailable")?;
    let document = window.document().ok_or("document unavailable")?;
    let reported = window.navigator().hardware_concurrency();
    let cpus = (reported.is_finite()
        && reported > 0.
        && reported.fract() == 0.
        && reported <= usize::MAX as f64)
        .then_some(reported as usize);
    let supported = Reflect::get(&js_sys::global(), &"Worker".into())?.is_function();
    let app = Rc::new(App {
        document,
        settings: WorkerSettings::new(cpus, supported),
        supported,
        running: RefCell::new(None),
        sequence: Cell::new(0),
        rows: Cell::new(0),
        listeners: RefCell::new(vec![]),
        copy_timers: RefCell::new(BTreeMap::new()),
    });
    app.update_rows()?;
    app.setup_workers(cpus)?;
    app.listen("number", "input", |app, _| app.update_rows())?;
    app.listen("alphabet", "change", |app, _| {
        app.update_alphabet();
        Ok(())
    })?;
    app.listen("reverse", "click", |app, _| {
        app.reverse();
        Ok(())
    })?;
    app.listen("search", "submit", |app, event| {
        event.prevent_default();
        app.submit()
    })?;
    app.listen("stop", "click", |app, _| {
        app.finish(Some("Stopped."));
        Ok(())
    })?;
    for name in ["a", "b"] {
        app.listen(&format!("copy_{name}"), "click", move |app, _| {
            app.copy(name);
            Ok(())
        })?;
    }
    app.text("log", "");
    app.busy(false);
    APP.with(|slot| *slot.borrow_mut() = Some(app));
    Ok(())
}
