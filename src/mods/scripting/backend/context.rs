use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use rhai::{Dynamic, Engine, EvalAltResult, Position};

use crate::scripting::api::ScriptContext;
use crate::scripting::events::{SCRIPT_EVENT_FIELD_NAMES, ScriptEvent};
use crate::scripting::value::ScriptValue;

thread_local! {
    static ACTIVE_CTX: RefCell<Option<ScriptContext>> = const { RefCell::new(None) };
}

pub(super) fn with_tls_context<R>(ctx: ScriptContext, f: impl FnOnce() -> R) -> (ScriptContext, R) {
    struct ResetOnDrop {
        previous: Option<ScriptContext>,
        completed: bool,
    }

    impl Drop for ResetOnDrop {
        fn drop(&mut self) {
            if self.completed {
                return;
            }
            ACTIVE_CTX.with(|slot| {
                if let Ok(mut borrow) = slot.try_borrow_mut() {
                    *borrow = self.previous.take();
                }
            });
        }
    }

    let previous = ACTIVE_CTX.with(|slot| slot.borrow_mut().replace(ctx));
    let mut guard = ResetOnDrop {
        previous,
        completed: false,
    };
    let result = f();
    let returned = ACTIVE_CTX
        .with(|slot| slot.borrow_mut().take())
        .expect("script TLS context should remain present");
    guard.completed = true;
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

pub(super) fn script_value_from_rhai(dynamic: &Dynamic) -> ScriptValue {
    if dynamic.is_unit() {
        return ScriptValue::Null;
    }
    if let Ok(v) = dynamic.as_bool() {
        return ScriptValue::Bool(v);
    }
    if let Ok(v) = dynamic.as_int() {
        return ScriptValue::Int(v);
    }
    if let Ok(v) = dynamic.as_float() {
        return ScriptValue::Float(v);
    }
    if let Ok(s) = dynamic.clone().into_string() {
        return ScriptValue::String(s);
    }
    if let Some(arr) = dynamic.read_lock::<rhai::Array>() {
        return ScriptValue::List(arr.iter().map(script_value_from_rhai).collect());
    }
    if let Some(map) = dynamic.read_lock::<rhai::Map>() {
        return ScriptValue::Map(
            map.iter()
                .map(|(k, v)| (k.to_string(), script_value_from_rhai(v)))
                .collect(),
        );
    }
    ScriptValue::String(dynamic.to_string())
}

pub(super) fn script_value_into_rhai(val: ScriptValue) -> Dynamic {
    match val {
        ScriptValue::Null => Dynamic::UNIT,
        ScriptValue::Bool(v) => Dynamic::from(v),
        ScriptValue::Int(v) => Dynamic::from(v),
        ScriptValue::Float(v) => Dynamic::from(v),
        ScriptValue::String(s) => Dynamic::from(s),
        ScriptValue::List(items) => {
            let arr: rhai::Array = items.into_iter().map(script_value_into_rhai).collect();
            Dynamic::from_array(arr)
        }
        ScriptValue::Map(map) => {
            let mut rhai_map = rhai::Map::new();
            for (k, v) in map {
                rhai_map.insert(k.into(), script_value_into_rhai(v));
            }
            Dynamic::from_map(rhai_map)
        }
    }
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
        self.with_inner(|e| e.has_aliased(key))
    }

    pub fn get(&mut self, key: &str) -> Dynamic {
        self.with_inner(|e| {
            e.get_aliased(key)
                .cloned()
                .map(script_value_into_rhai)
                .unwrap_or(Dynamic::UNIT)
        })
    }

    pub fn set(&mut self, key: &str, value: Dynamic) {
        let converted = script_value_from_rhai(&value);
        self.with_inner(|e| e.set_aliased(key, converted));
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

    for &prop in SCRIPT_EVENT_FIELD_NAMES {
        engine.register_get(prop, move |ev: &mut RhaiEvent| -> Dynamic { ev.get(prop) });
        engine.register_set(prop, move |ev: &mut RhaiEvent, val: Dynamic| ev.set(prop, val));
    }
}
