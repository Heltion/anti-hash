use super::{
    config,
    runtime::{Timer, WorkerHandle, copy_config, defer, set},
    ui::{App, DIRECTIONS},
};
use crate::{
    auto_length::{Outcome, Planner},
    clock::Clock,
};
use js_sys::{Array, Reflect};
use std::{collections::BTreeSet, rc::Rc};
use wasm_bindgen::prelude::*;

#[derive(Default)]
struct Slot {
    worker: Option<WorkerHandle>,
    ready: bool,
    generation: u64,
    timer: Option<Timer>,
    config: Option<JsValue>,
    length: usize,
    ticket: u64,
}
pub(super) struct Session {
    id: u64,
    config: JsValue,
    automatic: bool,
    manual_length: usize,
    workers: usize,
    clock: Clock,
    budget: f64,
    planner: Planner,
    slots: Vec<Slot>,
    pending: BTreeSet<usize>,
    outcomes: Vec<Outcome>,
    ticket: u64,
    best: Option<(usize, String, String)>,
    _deadline: Timer,
}
impl App {
    fn active(&self, id: u64) -> bool {
        self.running.borrow().as_ref().is_some_and(|s| s.id == id)
    }
    fn worker_active(&self, id: u64, index: usize, generation: u64) -> bool {
        self.running.borrow().as_ref().is_some_and(|session| {
            session.id == id
                && session
                    .slots
                    .get(index)
                    .is_some_and(|slot| slot.worker.is_some() && slot.generation == generation)
        })
    }
    pub fn submit(self: &Rc<Self>) -> Result<(), JsValue> {
        if self.running.borrow().is_some() {
            return Ok(());
        }
        let config = self.snapshot()?;
        let length = self.input("length").value().trim().to_owned();
        let automatic = length.eq_ignore_ascii_case("auto");
        let manual_length = if automatic {
            1
        } else {
            length
                .parse::<usize>()
                .ok()
                .filter(|&n| n > 0)
                .ok_or("Length must be auto or a positive integer.")?
        };
        let selected = self.select("workers").value();
        let workers = if !self.supported {
            0
        } else if selected == "auto" {
            self.settings
                .automatic
                .ok_or("Auto workers are unavailable.")?
        } else {
            selected
                .parse::<usize>()
                .ok()
                .filter(|&n| n <= self.settings.maximum)
                .ok_or("Worker count exceeds detected CPU count.")?
        };
        let validation = copy_config(&config)?;
        set(&validation, "length", manual_length.to_string())?;
        config::validate_config(&validation)?;
        let budget: f64 = self
            .input("timeout")
            .value()
            .parse()
            .map_err(|_| JsValue::from_str("Invalid time budget."))?;
        let id = self.sequence.get().wrapping_add(1);
        self.sequence.set(id);
        let weak = Rc::downgrade(self);
        let deadline = Timer::new(budget * 1000., move || {
            let weak = weak.clone();
            defer(move || {
                if let Some(app) = weak.upgrade()
                    && app.active(id)
                {
                    app.finish(Some("Time budget reached."));
                }
            });
        })?;
        *self.running.borrow_mut() = Some(Session {
            id,
            config,
            automatic,
            manual_length,
            workers,
            clock: Clock::start(),
            budget,
            planner: Planner::new(workers.max(1)),
            slots: vec![],
            pending: BTreeSet::new(),
            outcomes: vec![],
            ticket: 0,
            best: None,
            _deadline: deadline,
        });
        self.busy(true);
        self.hidden("outputs", true);
        self.text("log", "Searching…");
        if let Err(error) = self.next_wave(id) {
            self.fail(error);
        }
        Ok(())
    }
    fn next_wave(self: &Rc<Self>, id: u64) -> Result<(), JsValue> {
        let mut running = self.running.borrow_mut();
        let Some(session) = running.as_mut().filter(|s| s.id == id) else {
            return Ok(());
        };
        if session.clock.elapsed() >= session.budget {
            drop(running);
            self.finish(Some("Time budget reached."));
            return Ok(());
        }
        let lengths = if session.automatic {
            session.planner.wave()
        } else if session.planner.attempts == 0 {
            vec![session.manual_length]
        } else {
            vec![]
        };
        if lengths.is_empty() {
            drop(running);
            self.finish(None);
            return Ok(());
        }
        let phase = if session.planner.best.is_some() {
            "Refining"
        } else {
            "Expanding"
        };
        self.text(
            "search_progress",
            &if session.automatic {
                format!(
                    "{phase} · lengths {} · {} · {} probes completed",
                    lengths
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    worker_label(session.workers.min(lengths.len())),
                    session.planner.attempts
                )
            } else {
                format!(
                    "Fixed length {} · {}",
                    session.manual_length,
                    worker_label(session.workers.min(1))
                )
            },
        );
        session.pending = (0..lengths.len()).collect();
        session.outcomes.clear();
        session
            .slots
            .resize_with(session.slots.len().max(lengths.len()), Slot::default);
        for (index, length) in lengths.into_iter().enumerate() {
            session.ticket += 1;
            let ticket = session.ticket;
            let remaining = (session.budget - session.clock.elapsed()).max(0.000001);
            let slice = if session.automatic {
                remaining.min((session.budget / 16.).max(0.05))
            } else {
                remaining
            };
            let task = copy_config(&session.config)?;
            set(&task, "length", length.to_string())?;
            set(&task, "timeout", slice.to_string())?;
            set(&task, "ticket", ticket.to_string())?;
            let slot = &mut session.slots[index];
            slot.length = length;
            slot.ticket = ticket;
            slot.config = Some(task);
            let weak = Rc::downgrade(self);
            if session.workers == 0 {
                slot.timer = Some(Timer::new(0., move || {
                    let weak = weak.clone();
                    defer(move || {
                        if let Some(app) = weak.upgrade() {
                            app.local(id, index, ticket);
                        }
                    });
                })?);
                continue;
            }
            slot.timer = Some(Timer::new(
                (slice + 0.1).min(remaining) * 1000.,
                move || {
                    let weak = weak.clone();
                    defer(move || {
                        if let Some(app) = weak.upgrade() {
                            let values = Array::new();
                            values.push(&"timeout".into());
                            app.complete(id, index, ticket, values);
                        }
                    });
                },
            )?);
            if slot.worker.is_none() {
                let generation = ticket;
                slot.generation = generation;
                let weak_message = Rc::downgrade(self);
                let weak_error = Rc::downgrade(self);
                slot.ready = false;
                slot.worker = Some(WorkerHandle::new(
                    move |data| {
                        let weak = weak_message.clone();
                        defer(move || {
                            if let Some(app) = weak.upgrade()
                                && app.worker_active(id, index, generation)
                                && let Err(error) = app.message(id, index, data)
                                && app.active(id)
                            {
                                app.fail(error);
                            }
                        });
                    },
                    move |error| {
                        let weak = weak_error.clone();
                        defer(move || {
                            if let Some(app) = weak.upgrade()
                                && app.worker_active(id, index, generation)
                            {
                                app.fail(error);
                            }
                        });
                    },
                )?);
                slot.worker
                    .as_ref()
                    .unwrap()
                    .worker
                    .post_message(&wasm_bindgen::module())?;
            } else if slot.ready
                && let Some(worker) = &slot.worker
            {
                worker.worker.post_message(slot.config.as_ref().unwrap())?;
            }
        }
        Ok(())
    }
    fn message(self: &Rc<Self>, id: u64, index: usize, data: JsValue) -> Result<(), JsValue> {
        if data.as_string().as_deref() == Some("ready") {
            let mut running = self.running.borrow_mut();
            if let Some(session) = running.as_mut().filter(|s| s.id == id) {
                let slot = &mut session.slots[index];
                slot.ready = true;
                if let (Some(worker), Some(config)) = (&slot.worker, &slot.config) {
                    worker.worker.post_message(config)?;
                }
            }
        } else {
            let packet = Array::from(&data);
            if packet.get(0).as_string().as_deref() == Some("error") {
                return Err(packet.get(1));
            }
            let ticket = packet
                .get(0)
                .as_string()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or("Invalid worker ticket.")?;
            self.complete(id, index, ticket, Array::from(&packet.get(1)));
        }
        Ok(())
    }
    fn local(self: &Rc<Self>, id: u64, index: usize, ticket: u64) {
        let task = {
            let running = self.running.borrow();
            let Some(session) = running.as_ref().filter(|s| s.id == id) else {
                return;
            };
            let slot = &session.slots[index];
            if slot.ticket != ticket || !session.pending.contains(&index) {
                return;
            }
            let task = slot.config.clone().expect("active task");
            let remaining = (session.budget - session.clock.elapsed()).max(0.000001);
            let configured = Reflect::get(&task, &"timeout".into())
                .ok()
                .and_then(|n| n.as_string())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(remaining);
            if let Err(error) = set(&task, "timeout", configured.min(remaining).to_string()) {
                drop(running);
                self.fail(error);
                return;
            }
            task
        };
        match config::solve(&task) {
            Ok(values) => self.complete(id, index, ticket, values),
            Err(error) => self.fail(error),
        }
    }
    fn complete(self: &Rc<Self>, id: u64, index: usize, ticket: u64, values: Array) {
        let mut running = self.running.borrow_mut();
        let Some(session) = running.as_mut().filter(|s| s.id == id) else {
            return;
        };
        if !session.pending.contains(&index) || session.slots[index].ticket != ticket {
            return;
        }
        let status = values.get(0).as_string().unwrap_or_default();
        let outcome = match status.as_str() {
            "ok" => Outcome::Success,
            "not_found" => Outcome::Failed,
            "timeout" => Outcome::Timeout,
            _ => {
                let error = values.get(1);
                drop(running);
                self.fail(error);
                return;
            }
        };
        let length = session.slots[index].length;
        session.pending.remove(&index);
        session.outcomes.push(outcome);
        session.slots[index].timer = None;
        session.slots[index].config = None;
        if outcome == Outcome::Timeout {
            session.slots[index].worker = None;
        }
        if session.planner.record(length, outcome) {
            let (Some(a), Some(b)) = (values.get(2).as_string(), values.get(3).as_string()) else {
                drop(running);
                self.fail("Invalid worker strings.".into());
                return;
            };
            self.text("string_a", &a);
            self.text("string_b", &b);
            self.text("reverse_description", DIRECTIONS[0]);
            self.hidden("outputs", false);
            session.best = Some((length, a, b));
            let cancelled: Vec<_> = session
                .pending
                .iter()
                .copied()
                .filter(|&i| session.slots[i].length > length)
                .collect();
            for index in cancelled {
                let slot = &mut session.slots[index];
                slot.worker = None;
                slot.timer = None;
                slot.config = None;
                session.pending.remove(&index);
                session.planner.record(slot.length, Outcome::Cancelled);
                session.outcomes.push(Outcome::Cancelled);
            }
        }
        if session.pending.is_empty() {
            session.planner.finish_wave(&session.outcomes);
            drop(running);
            if let Err(error) = self.next_wave(id) {
                self.fail(error);
            }
        }
    }
    pub fn finish(&self, prefix: Option<&str>) {
        let Some(mut session) = self.running.borrow_mut().take() else {
            return;
        };
        for &index in &session.pending {
            if session.slots[index].config.is_some() {
                session
                    .planner
                    .record(session.slots[index].length, Outcome::Cancelled);
            }
        }
        let elapsed = session.clock.elapsed();
        let timed_out = elapsed >= session.budget
            || (!session.automatic && session.outcomes.contains(&Outcome::Timeout));
        let prefix = prefix.unwrap_or(if timed_out {
            "Time budget reached."
        } else {
            ""
        });
        let prefix = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix} ")
        };
        self.text(
            "log",
            &if let Some((length, _, _)) = &session.best {
                format!(
                    "{prefix}{}: length {length} · {elapsed:.3}s{}",
                    if session.automatic {
                        "Shortest found"
                    } else {
                        "Collision found"
                    },
                    if session.automatic {
                        " · heuristic search"
                    } else {
                        ""
                    }
                )
            } else {
                format!(
                    "{prefix}No collision found · {elapsed:.3}s{}",
                    if session.planner.stalled {
                        " · probes reached their time budget"
                    } else {
                        ""
                    }
                )
            },
        );
        self.text(
            "search_progress",
            &format!(
                "{} total probes · {}{}",
                session.planner.attempts,
                worker_label(if session.automatic {
                    session.workers
                } else {
                    session.workers.min(1)
                }),
                session
                    .best
                    .as_ref()
                    .filter(|_| session.automatic)
                    .map(|(length, _, _)| format!(" · shortest found {length}"))
                    .unwrap_or_default()
            ),
        );
        drop(session);
        self.busy(false);
    }
    fn fail(&self, error: JsValue) {
        let retained = self
            .running
            .borrow()
            .as_ref()
            .and_then(|s| s.best.as_ref().map(|(n, _, _)| *n));
        self.finish(None);
        let mut message = error.as_string().unwrap_or_else(|| format!("{error:?}"));
        if let Some(length) = retained {
            message += &format!(" · retaining length {length}");
        }
        self.text("log", &message);
    }
}
fn worker_label(count: usize) -> String {
    if count == 0 {
        "main thread".into()
    } else {
        format!("up to {count} worker{}", if count == 1 { "" } else { "s" })
    }
}
