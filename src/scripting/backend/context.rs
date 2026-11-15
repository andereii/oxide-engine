use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use rhai::{Dynamic, Engine, EvalAltResult, Position};

use crate::scripting::api::ScriptContext;
use crate::scripting::events::ScriptEvent;
use crate::scripting::value::ScriptValue;

thread_local! {
    static ACTIVE_CTX: RefCell<Option<ScriptContext>> = const { RefCell::new(None) };
}

pub(super) fn with_tls_context<R>(ctx: ScriptContext, f: impl FnOnce() -> R) -> (ScriptContext, R) {
    struct ResetOnDrop {
        previous: Option<ScriptContext>,
    }

    impl Drop for ResetOnDrop {
        fn drop(&mut self) {
            if let Some(prev) = self.previous.take() {
                ACTIVE_CTX.with(|slot| {
                    *slot.borrow_mut() = Some(prev);
                });
            }
        }
    }

    let previous = ACTIVE_CTX.with(|slot| slot.borrow_mut().replace(ctx));
    let mut guard = ResetOnDrop { previous };
    let result = f();
    let returned = ACTIVE_CTX
        .with(|slot| slot.borrow_mut().take())
        .expect("script TLS context should remain present");
    if let Some(prev) = guard.previous.take() {
        ACTIVE_CTX.with(|slot| {
            *slot.borrow_mut() = Some(prev);
        });
    }
    (returned, result)
}

pub(super) fn runtime_err(msg: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(msg.into().into(), Position::NONE))
}

pub(super) fn with_ctx<R>(f: impl FnOnce(&mut ScriptContext) -> R) -> Result<R, Box<EvalAltResult>> {
    ACTIVE_CTX.with(|slot| {
        let mut borrow = slot
            .try_borrow_mut()
            .map_err(|_| runtime_err("re-entrant script context borrow"))?;
        let ctx = borrow
            .as_mut()
            .ok_or_else(|| runtime_err("script API called outside active ScriptContext"))?;
        Ok(f(ctx))
    })
}

pub(super) fn dyn_to_f64(val: &Dynamic) -> Result<f64, Box<EvalAltResult>> {
    if let Ok(v) = val.as_float() {
        return Ok(v);
    }
    if let Ok(v) = val.as_int() {
        return Ok(v as f64);
    }
    Err(runtime_err(format!("expected numeric value, got {}", val.type_name())))
}

pub(super) fn dyn_to_i64(val: &Dynamic) -> Result<i64, Box<EvalAltResult>> {
    if let Ok(v) = val.as_int() {
        return Ok(v);
    }
    if let Ok(v) = val.as_float() {
        return Ok(v as i64);
    }
    Err(runtime_err(format!("expected integer value, got {}", val.type_name())))
}

pub(super) fn dyn_to_usize(val: &Dynamic) -> Result<usize, Box<EvalAltResult>> {
    let i = dyn_to_i64(val)?;
    usize::try_from(i).map_err(|_| runtime_err(format!("expected non-negative index, got {i}")))
}

#[derive(Clone)]
pub struct RhaiEvent {
    inner: Arc<Mutex<ScriptEvent>>,
}

impl RhaiEvent {
    pub(super) fn from_event(event: ScriptEvent) -> Self {
        Self {
            inner: Arc::new(Mutex::new(event)),
        }
    }

    pub(super) fn snapshot_into(&self, target: &mut ScriptEvent) {
        if let Ok(guard) = self.inner.lock() {
            *target = guard.clone();
        }
    }

    fn with_inner<R>(&self, f: impl FnOnce(&mut ScriptEvent) -> R) -> R {
        match self.inner.lock() {
            Ok(mut guard) => f(&mut guard),
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                f(&mut guard)
            }
        }
    }

    pub fn cancel(&mut self) {
        self.with_inner(ScriptEvent::cancel);
    }

    pub fn stop(&mut self) {
        self.with_inner(ScriptEvent::stop_propagation);
    }

    pub fn is_canceled(&mut self) -> bool {
        self.with_inner(|e| e.canceled)
    }

    pub fn kind_name(&mut self) -> String {
        self.with_inner(|e| e.kind.canonical_name().to_string())
    }

    pub fn has(&mut self, key: &str) -> bool {
        self.with_inner(|e| e.data.contains_key(key))
    }

    pub fn get(&mut self, key: &str) -> Dynamic {
        self.with_inner(|e| {
            let alias = match key {
                "note_kind" => Some("kind"),
                "hold_len" => Some("length"),
                "lane" => Some("dir"),
                _ => None,
            };
            e.data
                .get(key)
                .or_else(|| alias.and_then(|k| e.data.get(k)))
                .cloned()
                .map(ScriptValue::into_rhai)
                .unwrap_or(Dynamic::UNIT)
        })
    }

    pub fn set(&mut self, key: &str, value: Dynamic) {
        let converted = ScriptValue::from_rhai(&value);
        self.with_inner(|e| e.set(key.to_string(), converted));
    }
}

pub(super) fn register_event_type(engine: &mut Engine) {
    engine.register_type_with_name::<RhaiEvent>("ScriptEvent");
    engine.register_fn("cancel", RhaiEvent::cancel);
    engine.register_fn("stop", RhaiEvent::stop);
    engine.register_fn("stop_propagation", RhaiEvent::stop);
    engine.register_fn("is_canceled", RhaiEvent::is_canceled);
    engine.register_get("canceled", RhaiEvent::is_canceled);
    engine.register_get("cancelled", RhaiEvent::is_canceled);
    engine.register_get("kind", RhaiEvent::kind_name);
    engine.register_fn("has", RhaiEvent::has);
    engine.register_fn("get", RhaiEvent::get);
    engine.register_fn("set", RhaiEvent::set);
    engine.register_indexer_get(|ev: &mut RhaiEvent, key: &str| -> Dynamic { ev.get(key) });
    engine.register_indexer_set(|ev: &mut RhaiEvent, key: &str, val: Dynamic| ev.set(key, val));

    for prop in [
        "id", "dir", "note_kind", "length", "hold_len", "combo", "is_hold", "remaining_ms",
        "health_change", "score_change", "score", "hp", "step", "beat", "measure", "dt", "name",
        "value", "data", "source", "judgement", "lane", "char_id", "time", "count", "is_player",
    ] {
        engine.register_get(prop, move |ev: &mut RhaiEvent| -> Dynamic { ev.get(prop) });
        engine.register_set(prop, move |ev: &mut RhaiEvent, val: Dynamic| ev.set(prop, val));
    }
}

