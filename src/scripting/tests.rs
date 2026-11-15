use std::collections::{BTreeMap, HashSet};
use std::f64::consts::TAU;
use std::path::Path;
use std::path::PathBuf;

use crate::mods::{ModRegistry, ModStore, ScriptScope};
use crate::utils::constants::{
    DEFAULT_BPM, DEFAULT_MOD_PRIORITY, HEALTH_MAX, HEALTH_STARTING, MS_PER_SEC, SCRIPT_EXT_LUA,
    SCRIPT_EXT_RHAI, SECS_PER_MIN, STEPS_PER_BEAT,
};

use super::api::{MAX_CUSTOM_HOOK_DEPTH, SCRIPT_API_CATALOG, ScriptApiCatalogEntry};
use super::{
    HookKind, LogLevel, ScriptBackend, ScriptCommand, ScriptContext, ScriptError, ScriptEvent,
    ScriptHost, ScriptInstance, ScriptLanguage, ScriptSource, ScriptStateSnapshot, ScriptValue,
};

struct MockLuaBackend;

struct MockLuaInstance {
    id: String,
    mod_id: String,
    scope: ScriptScope,
    priority: i32,
    bonus: f64,
}

impl ScriptBackend for MockLuaBackend {
    fn language(&self) -> ScriptLanguage {
        ScriptLanguage::Lua
    }

    fn extensions(&self) -> &[&str] {
        &[SCRIPT_EXT_LUA]
    }

    fn compile(&self, source: ScriptSource) -> Result<Box<dyn ScriptInstance>, ScriptError> {
        let bonus = source.code.trim().parse::<f64>().unwrap_or(0.0);
        Ok(Box::new(MockLuaInstance {
            id: source.id,
            mod_id: source.mod_id,
            scope: source.scope,
            priority: source.priority,
            bonus,
        }))
    }
}

impl ScriptInstance for MockLuaInstance {
    fn id(&self) -> &str {
        &self.id
    }

    fn mod_id(&self) -> &str {
        &self.mod_id
    }

    fn scope(&self) -> &ScriptScope {
        &self.scope
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn language(&self) -> &ScriptLanguage {
        &ScriptLanguage::Lua
    }

    fn path(&self) -> Option<&Path> {
        None
    }

    fn has_hook(&self, hook: &HookKind) -> bool {
        matches!(hook, HookKind::BeatHit)
    }

    fn call_hook(
        &mut self,
        hook: &HookKind,
        event: &mut ScriptEvent,
        ctx: &mut ScriptContext,
    ) -> Result<ScriptValue, ScriptError> {
        if matches!(hook, HookKind::BeatHit) {
            ctx.mutable.score += self.bonus;
            ctx.mutable.dirty_score = true;
            event.set("lua_seen", true);
        }
        Ok(ScriptValue::Bool(true))
    }

    fn call_fn(
        &mut self,
        _fn_name: &str,
        _args: &[ScriptValue],
        _ctx: &mut ScriptContext,
    ) -> Result<ScriptValue, ScriptError> {
        Ok(ScriptValue::Null)
    }
}

fn empty_registry() -> ModRegistry {
    ModRegistry::discover(PathBuf::new(), PathBuf::new())
}

#[test]
fn supports_pluggable_future_backends_alongside_rhai() {
    let mut host = ScriptHost::with_default_backends();
    host.register_backend(Box::new(MockLuaBackend));

    let exts = host.supported_extensions();
    assert!(exts.iter().any(|e| e == SCRIPT_EXT_RHAI));
    assert!(exts.iter().any(|e| e == SCRIPT_EXT_LUA));

    let lua_bonus = 75.0;
    host.load_source(
        SCRIPT_EXT_LUA,
        ScriptSource::inline("custom:lua_mod", "custom", lua_bonus.to_string()),
    )
    .unwrap();
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "rhai_mod:beat",
            "rhai_mod",
            "fn beat(e) { if e.get(\"lua_seen\") == true { add_score(25); } }",
        ),
    )
    .unwrap();

    let reg = empty_registry();
    let mut store = ModStore::default();
    let mut ctx = host.make_context(ScriptStateSnapshot::default(), &reg, &store);
    let mut ev = ScriptEvent::new(HookKind::BeatHit).with_field("beat", 1_i64);

    host.dispatch(&HookKind::BeatHit, &mut ev, &mut ctx);
    let ctx = host.finalize_context(ctx, &mut store);

    assert!(ev.get_bool("lua_seen") == Some(true));
    assert_eq!(ctx.mutable.score, lua_bonus + 25.0);
}

#[test]
fn rhai_short_aliases_camel_case_cancel_stop_and_this_persistence() {
    let mut host = ScriptHost::with_default_backends();
    let reg = empty_registry();
    let mut store = ModStore::default();

    // Script 1 uses short alias `hit(e)`, persistent `this`, and mutates event/cancels default miss.
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "mod1:main",
            "mod1",
            r#"
            set_zoom(1.25);

            fn hit(e) {
                this.hits = (this.hits ?? 0) + 1;
                e.score = 1234;
                set_hp(hp() + 0.1);
                if this.hits >= 2 {
                    e.stop();
                }
            }

            fn onNoteMiss(e) {
                e.cancel();
            }
            "#,
        ),
    )
    .unwrap();

    // Script 2 uses camelCase alias `onNoteHit(e)` and lower priority so stop() blocks it on 2nd hit.
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource {
            id: "mod2:later".to_string(),
            mod_id: "mod2".to_string(),
            name: "later".to_string(),
            scope: ScriptScope::Song("test".to_string()),
            priority: DEFAULT_MOD_PRIORITY + 1,
            path: None,
            code: r#"
                fn onNoteHit(e) {
                    add_score(500);
                }
            "#
            .to_string(),
        },
    )
    .unwrap();

    let mut snap = ScriptStateSnapshot::default();
    snap.health = HEALTH_STARTING;

    // Create initializes top-level statements (`set_zoom(1.25)`).
    let mut ctx = host.make_context(snap.clone(), &reg, &store);
    let mut create_ev = ScriptEvent::new(HookKind::Create);
    host.dispatch(&HookKind::Create, &mut create_ev, &mut ctx);
    assert!((ctx.mutable.cam_zoom - 1.25).abs() < 1e-6);
    snap.cam_zoom = ctx.mutable.cam_zoom;
    snap.health = ctx.mutable.health;

    // 1st NoteHit propagates to mod2 (`+500` score).
    let mut hit1 = ScriptEvent::new(HookKind::NoteHit).with_field("dir", 0_i64);
    host.dispatch(&HookKind::NoteHit, &mut hit1, &mut ctx);
    assert_eq!(hit1.get_i64("score"), Some(1234));
    assert!((ctx.mutable.score - 500.0).abs() < 1e-6);
    assert!((ctx.mutable.health - (HEALTH_STARTING + 0.1).min(HEALTH_MAX)).abs() < 1e-6);

    // 2nd NoteHit stops propagation before mod2 runs.
    let mut hit2 = ScriptEvent::new(HookKind::NoteHit).with_field("dir", 1_i64);
    host.dispatch(&HookKind::NoteHit, &mut hit2, &mut ctx);
    assert!(hit2.propagation_stopped);
    assert!((ctx.mutable.score - 500.0).abs() < 1e-6);

    // NoteMiss is canceled by `onNoteMiss`.
    let mut miss = ScriptEvent::new(HookKind::NoteMiss);
    host.dispatch(&HookKind::NoteMiss, &mut miss, &mut ctx);
    assert!(miss.canceled);

    host.finalize_context(ctx, &mut store);
}

#[test]
fn modchart_lanes_tweens_timers_and_store_work_end_to_end() {
    let mut host = ScriptHost::with_default_backends();
    let reg = empty_registry();
    let mut store = ModStore::default();

    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "fx:modchart",
            "fx",
            r##"
            fn create() {
                lane_pos(4, 20, -10);
                lane_angle(4, 45);
                cam_turn(15, 0.5, "linear");
                tween_lane(4, "x", 60, 0.5, "linear");
                spawn_rect("fade", 1280, 720, "#FF00FF", 0, 0, 10);
                after(0.25, "on_ready");
            }

            fn on_ready() {
                save_set("ready_ran", true);
                shake(6, 0.2);
            }
            "##,
        ),
    )
    .unwrap();

    let mut ctx = host.make_context(ScriptStateSnapshot::default(), &reg, &store);
    let mut create_ev = ScriptEvent::new(HookKind::Create);
    host.dispatch(&HookKind::Create, &mut create_ev, &mut ctx);

    assert_eq!(ctx.snapshot.lane_offsets.get(&4), Some(&[20.0, -10.0]));
    assert_eq!(ctx.snapshot.lane_angles.get(&4), Some(&45.0));
    assert!(ctx
        .commands
        .iter()
        .any(|c| matches!(c, ScriptCommand::SpawnProp { id, .. } if id == "fade")));

    // Halfway through 0.5s tweens + after(0.25s) timer triggers `on_ready`.
    host.tick_timers_and_tweens(0.25, &mut ctx);
    assert!((ctx.mutable.cam_angle_deg - 7.5).abs() < 1e-3);
    assert!((ctx.snapshot.lane_offsets.get(&4).unwrap()[0] - 40.0).abs() < 1e-3);
    assert!(ctx
        .commands
        .iter()
        .any(|c| matches!(c, ScriptCommand::CameraShake { .. })));

    // Finish tweens at 0.5s.
    host.tick_timers_and_tweens(0.25, &mut ctx);
    assert!((ctx.mutable.cam_angle_deg - 15.0).abs() < 1e-3);
    assert!((ctx.snapshot.lane_offsets.get(&4).unwrap()[0] - 60.0).abs() < 1e-3);

    let _ = host.finalize_context(ctx, &mut store);
    assert_eq!(store.get("fx", "ready_ran").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn script_api_catalog_entries_and_abi_contract_are_valid() {
    assert!(!SCRIPT_API_CATALOG.is_empty());

    let mut seen_names = HashSet::new();
    for entry in SCRIPT_API_CATALOG {
        let entry: &ScriptApiCatalogEntry = entry;
        assert!(!entry.name.is_empty(), "catalog entry name must not be empty");
        assert_eq!(
            entry.name,
            entry.name.trim(),
            "catalog entry name {:?} has surrounding whitespace",
            entry.name
        );
        assert!(
            entry.name.starts_with(|c: char| c.is_ascii_lowercase())
                && entry
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "catalog entry name '{}' must be lowercase snake_case",
            entry.name
        );
        assert!(
            seen_names.insert(entry.name),
            "duplicate catalog entry name '{}'",
            entry.name
        );

        assert!(
            !entry.signature.is_empty(),
            "catalog entry '{}' has empty signature",
            entry.name
        );
        assert_eq!(entry.signature, entry.signature.trim());
        assert!(
            entry.signature.contains(entry.name)
                && entry.signature.contains('(')
                && entry.signature.contains(')'),
            "catalog entry '{}' signature '{}' must document call syntax",
            entry.name,
            entry.signature
        );

        assert!(
            !entry.category.is_empty(),
            "catalog entry '{}' has empty category",
            entry.name
        );
        assert_eq!(entry.category, entry.category.trim());
        assert!(
            entry
                .category
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_'),
            "catalog entry '{}' category '{}' must be lowercase",
            entry.name,
            entry.category
        );

        assert!(
            !entry.summary.is_empty(),
            "catalog entry '{}' has empty summary",
            entry.name
        );
        assert_eq!(entry.summary, entry.summary.trim());
    }

    for required in [
        "emit", "beat_f", "step_f", "lerp", "clamp", "remap", "sin_beat", "cos_beat",
    ] {
        assert!(
            seen_names.contains(required),
            "SCRIPT_API_CATALOG is missing required symbol '{required}'"
        );
    }

    let custom_name = "abi_probe";
    let custom_hook = HookKind::Custom(custom_name.to_string());
    assert_eq!(custom_hook.canonical_name(), custom_name);
    assert_eq!(custom_hook.candidate_names(), vec![custom_name]);

    let reg = empty_registry();
    let mut store = ModStore::default();
    let mut host = ScriptHost::with_default_backends();
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "abi:probe",
            "abi",
            r#"
            fn create() {
                let bf = beat_f();
                let sf = step_f();
                let l = lerp(0.0, 10.0, 0.5);
                let c = clamp(15.0, 0.0, 10.0);
                let r = remap(5.0, 0.0, 10.0, 0.0, 100.0);
                let sb = sin_beat(1.0, 4.0);
                let cb = cos_beat(1.0, 4.0);
                emit("abi_ping");
                emit("abi_pong", #{ bf: bf, sf: sf, l: l, c: c, r: r, sb: sb, cb: cb });
            }

            fn abi_ping(e) {
                save_set("ping_ok", e.kind == "abi_ping" && e.name == "abi_ping");
            }

            fn abi_pong(e) {
                save_set("pong_l", e.data.l);
                save_set("pong_c", e.value.c);
                save_set("pong_r", e.value.r);
            }
            "#,
        ),
    )
    .unwrap();

    let mut ctx = host.make_context(ScriptStateSnapshot::default(), &reg, &store);
    ctx.current_script_id = "manual".to_string();
    ctx.current_mod_id = "abi".to_string();
    ctx.emit_custom_hook(custom_name, ScriptValue::Int(7));
    assert_eq!(ctx.pending_custom_hooks.len(), 1);
    assert_eq!(ctx.pending_custom_hooks[0].sender_script_id, "manual");
    assert_eq!(ctx.pending_custom_hooks[0].sender_mod_id, "abi");
    ctx.pending_custom_hooks.clear();

    let mut create_ev = ScriptEvent::new(HookKind::Create);
    host.dispatch(&HookKind::Create, &mut create_ev, &mut ctx);
    let _ = host.finalize_context(ctx, &mut store);

    assert!(
        host.runtime_errors.is_empty(),
        "unexpected runtime errors: {:?}",
        host.runtime_errors
    );
    assert_eq!(store.get("abi", "ping_ok").and_then(|v| v.as_bool()), Some(true));
    assert!(
        (store.get("abi", "pong_l").and_then(|v| v.as_f64()).unwrap() - 5.0).abs() < 1e-9
    );
    assert!(
        (store.get("abi", "pong_c").and_then(|v| v.as_f64()).unwrap() - 10.0).abs() < 1e-9
    );
    assert!(
        (store.get("abi", "pong_r").and_then(|v| v.as_f64()).unwrap() - 50.0).abs() < 1e-9
    );
}

#[test]
fn custom_hooks_emit_across_scripts_and_timers_and_bound_recursion() {
    let mut host = ScriptHost::with_default_backends();
    let reg = empty_registry();
    let mut store = ModStore::default();

    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource {
            id: "mod_emit:sender".to_string(),
            mod_id: "mod_emit".to_string(),
            name: "sender".to_string(),
            scope: ScriptScope::Global,
            priority: DEFAULT_MOD_PRIORITY,
            path: None,
            code: r#"
                fn create() {
                    after(0.1, "fire_delayed");
                }

                fn beat(e) {
                    emit("notify_bare");
                    emit("phase_shift", #{ phase: 2, bonus: 150.0, label: "drop" });
                }

                fn fire_delayed() {
                    emit("timer_custom", 42);
                }
            "#
            .to_string(),
        },
    )
    .unwrap();

    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource {
            id: "mod_recv:listener".to_string(),
            mod_id: "mod_recv".to_string(),
            name: "listener".to_string(),
            scope: ScriptScope::Global,
            priority: DEFAULT_MOD_PRIORITY + 1,
            path: None,
            code: r#"
                fn notify_bare(e) {
                    save_set("bare_name", e.name);
                    save_set("bare_kind", e.kind);
                    save_set("bare_source", e.source);
                    save_set("bare_is_unit", e.value == () && e.data == ());
                }

                fn phase_shift(e) {
                    add_score(e.value.bonus);
                    save_set("phase", e.data.phase);
                    save_set("label", e.value.label);
                    e.stop();
                }

                fn timer_custom(e) {
                    save_set("timer_val", e.value);
                }
            "#
            .to_string(),
        },
    )
    .unwrap();

    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource {
            id: "mod_tail:tail".to_string(),
            mod_id: "mod_tail".to_string(),
            name: "tail".to_string(),
            scope: ScriptScope::Global,
            priority: DEFAULT_MOD_PRIORITY + 2,
            path: None,
            code: r#"
                fn phase_shift(e) {
                    save_set("tail_ran", true);
                }
            "#
            .to_string(),
        },
    )
    .unwrap();

    let mut ctx = host.make_context(ScriptStateSnapshot::default(), &reg, &store);
    let mut create_ev = ScriptEvent::new(HookKind::Create);
    host.dispatch(&HookKind::Create, &mut create_ev, &mut ctx);

    let mut beat_ev = ScriptEvent::new(HookKind::BeatHit).with_field("beat", 1_i64);
    host.dispatch(&HookKind::BeatHit, &mut beat_ev, &mut ctx);
    assert!((ctx.mutable.score - 150.0).abs() < 1e-9);

    host.tick_timers_and_tweens(0.1, &mut ctx);
    let _ = host.finalize_context(ctx, &mut store);

    assert!(
        host.runtime_errors.is_empty(),
        "unexpected runtime errors: {:?}",
        host.runtime_errors
    );
    assert_eq!(
        store.get("mod_recv", "bare_name").and_then(|v| v.as_str()),
        Some("notify_bare")
    );
    assert_eq!(
        store.get("mod_recv", "bare_kind").and_then(|v| v.as_str()),
        Some("notify_bare")
    );
    assert_eq!(
        store.get("mod_recv", "bare_source").and_then(|v| v.as_str()),
        Some("mod_emit:sender")
    );
    assert_eq!(
        store.get("mod_recv", "bare_is_unit").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        store.get("mod_recv", "phase").and_then(|v| v.as_i64()),
        Some(2)
    );
    assert_eq!(
        store.get("mod_recv", "label").and_then(|v| v.as_str()),
        Some("drop")
    );
    assert_eq!(
        store.get("mod_recv", "timer_val").and_then(|v| v.as_i64()),
        Some(42)
    );
    assert!(store.get("mod_tail", "tail_ran").is_none());

    // Infinite self-trigger loop must terminate cleanly at MAX_CUSTOM_HOOK_DEPTH.
    let mut guard_host = ScriptHost::with_default_backends();
    let mut guard_store = ModStore::default();
    guard_host
        .load_source(
            SCRIPT_EXT_RHAI,
            ScriptSource::inline(
                "guard:loop",
                "guard",
                r#"
                fn create() {
                    emit("self_loop", 1);
                }

                fn self_loop(e) {
                    this.runs = (this.runs ?? 0) + 1;
                    save_set("runs", this.runs);
                    emit("self_loop", this.runs + 1);
                }
                "#,
            ),
        )
        .unwrap();

    let mut guard_ctx = guard_host.make_context(ScriptStateSnapshot::default(), &reg, &guard_store);
    let mut guard_ev = ScriptEvent::new(HookKind::Create);
    guard_host.dispatch(&HookKind::Create, &mut guard_ev, &mut guard_ctx);
    let guard_ctx = guard_host.finalize_context(guard_ctx, &mut guard_store);

    assert_eq!(
        guard_store.get("guard", "runs").and_then(|v| v.as_i64()),
        Some(MAX_CUSTOM_HOOK_DEPTH as i64)
    );
    assert!(guard_ctx.pending_custom_hooks.is_empty());
    assert!(!guard_host.runtime_errors.is_empty());
    assert!(guard_ctx.commands.iter().any(|cmd| matches!(
        cmd,
        ScriptCommand::Log { level: LogLevel::Error, mod_id, message }
        if mod_id == "guard" && message.contains(&MAX_CUSTOM_HOOK_DEPTH.to_string())
    )));
}

#[test]
fn modchart_and_gameplay_math_helpers_match_analytical_snapshot_formulas() {
    let mut host = ScriptHost::with_default_backends();
    let reg = empty_registry();
    let mut store = ModStore::default();

    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "math:probe",
            "math",
            r#"
            fn eval_all(p) {
                lane_y(0, sin_beat(p.mult, p.amp));
                #{
                    beat_f: beat_f(),
                    step_f: step_f(),
                    sin_beat: sin_beat(p.mult, p.amp),
                    cos_beat: cos_beat(p.mult, p.amp),
                    lerp: lerp(p.lerp_a, p.lerp_b, p.lerp_t),
                    clamp_mid: clamp(p.mid, p.in_lo, p.in_hi),
                    clamp_lo: clamp(p.below, p.in_lo, p.in_hi),
                    clamp_hi: clamp(p.above, p.in_lo, p.in_hi),
                    clamp_swapped: clamp(p.above, p.in_hi, p.in_lo),
                    remap: remap(p.remap_v, p.in_lo, p.in_hi, p.out_lo, p.out_hi),
                    remap_degenerate: remap(p.remap_v, p.in_lo, p.in_lo, p.out_lo, p.out_hi),
                }
            }
            "#,
        ),
    )
    .unwrap();

    let default_snap = ScriptStateSnapshot {
        song_position_ms: ((SECS_PER_MIN / DEFAULT_BPM) * MS_PER_SEC) * 1.375,
        ..ScriptStateSnapshot::default()
    };

    let custom_bpm = DEFAULT_BPM * 1.45;
    let custom_beat_ms = (SECS_PER_MIN / custom_bpm) * MS_PER_SEC;
    let custom_step_ms = custom_beat_ms / f64::from(STEPS_PER_BEAT);
    let custom_snap = ScriptStateSnapshot {
        bpm: custom_bpm,
        beat_length_ms: custom_beat_ms,
        step_length_ms: custom_step_ms,
        song_position_ms: custom_beat_ms * 3.625,
        ..ScriptStateSnapshot::default()
    };

    let zero_snap = ScriptStateSnapshot {
        beat_length_ms: 0.0,
        step_length_ms: 0.0,
        song_position_ms: custom_beat_ms,
        ..ScriptStateSnapshot::default()
    };

    for snap in [default_snap, custom_snap, zero_snap] {
        let expected_beat_f = if snap.beat_length_ms > 0.0 {
            snap.song_position_ms / snap.beat_length_ms
        } else {
            0.0
        };
        let expected_step_f = if snap.step_length_ms > 0.0 {
            snap.song_position_ms / snap.step_length_ms
        } else {
            expected_beat_f * f64::from(STEPS_PER_BEAT)
        };
        let mult = 0.75_f64;
        let amp = 28.0_f64;
        let expected_sin = (expected_beat_f * mult * TAU).sin() * amp;
        let expected_cos = (expected_beat_f * mult * TAU).cos() * amp;

        let lerp_a = -12.0_f64;
        let lerp_b = 36.0_f64;
        let lerp_t = 0.35_f64;
        let expected_lerp = lerp_a + (lerp_b - lerp_a) * lerp_t;

        let in_lo = 10.0_f64;
        let in_hi = 50.0_f64;
        let out_lo = -100.0_f64;
        let out_hi = 100.0_f64;
        let remap_v = 25.0_f64;
        let expected_remap = out_lo + (out_hi - out_lo) * ((remap_v - in_lo) / (in_hi - in_lo));
        let mid = (in_lo + in_hi) * 0.5;
        let below = in_lo - 15.0;
        let above = in_hi + 40.0;

        let mut params = BTreeMap::new();
        params.insert("mult".to_string(), ScriptValue::Float(mult));
        params.insert("amp".to_string(), ScriptValue::Float(amp));
        params.insert("lerp_a".to_string(), ScriptValue::Float(lerp_a));
        params.insert("lerp_b".to_string(), ScriptValue::Float(lerp_b));
        params.insert("lerp_t".to_string(), ScriptValue::Float(lerp_t));
        params.insert("in_lo".to_string(), ScriptValue::Float(in_lo));
        params.insert("in_hi".to_string(), ScriptValue::Float(in_hi));
        params.insert("out_lo".to_string(), ScriptValue::Float(out_lo));
        params.insert("out_hi".to_string(), ScriptValue::Float(out_hi));
        params.insert("remap_v".to_string(), ScriptValue::Float(remap_v));
        params.insert("mid".to_string(), ScriptValue::Float(mid));
        params.insert("below".to_string(), ScriptValue::Float(below));
        params.insert("above".to_string(), ScriptValue::Float(above));

        let mut ctx = host.make_context(snap, &reg, &store);
        let result = host
            .call_fn_on_script("math:probe", "eval_all", &[ScriptValue::Map(params)], &mut ctx)
            .expect("eval_all should return map");
        let ScriptValue::Map(out) = result else {
            panic!("expected ScriptValue::Map from eval_all");
        };

        assert!(
            host.runtime_errors.is_empty(),
            "unexpected runtime errors: {:?}",
            host.runtime_errors
        );

        let get_f64 = |k: &str| out.get(k).and_then(ScriptValue::as_f64).unwrap();
        assert!((get_f64("beat_f") - expected_beat_f).abs() < 1e-12);
        assert!((get_f64("step_f") - expected_step_f).abs() < 1e-12);
        assert!((get_f64("step_f") - expected_beat_f * f64::from(STEPS_PER_BEAT)).abs() < 1e-12);
        assert!((get_f64("sin_beat") - expected_sin).abs() < 1e-12);
        assert!((get_f64("cos_beat") - expected_cos).abs() < 1e-12);
        assert!((get_f64("lerp") - expected_lerp).abs() < 1e-12);
        assert!((get_f64("clamp_mid") - mid).abs() < 1e-12);
        assert!((get_f64("clamp_lo") - in_lo).abs() < 1e-12);
        assert!((get_f64("clamp_hi") - in_hi).abs() < 1e-12);
        assert!((get_f64("clamp_swapped") - in_hi).abs() < 1e-12);
        assert!((get_f64("remap") - expected_remap).abs() < 1e-12);
        assert!((get_f64("remap_degenerate") - out_lo).abs() < 1e-12);

        let lane_offset = ctx.snapshot.lane_offsets.get(&0).copied().unwrap();
        assert!((lane_offset[0] - 0.0).abs() < 1e-6);
        assert!((lane_offset[1] - expected_sin as f32).abs() < 1e-4);

        let _ = host.finalize_context(ctx, &mut store);
    }
}
