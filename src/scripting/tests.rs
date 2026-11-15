use std::path::Path;
use std::path::PathBuf;

use crate::mods::{ModRegistry, ModStore, ScriptScope};
use crate::utils::constants::{
    DEFAULT_MOD_PRIORITY, HEALTH_MAX, HEALTH_STARTING, SCRIPT_EXT_LUA, SCRIPT_EXT_RHAI,
};

use super::{
    HookKind, ScriptBackend, ScriptCommand, ScriptContext, ScriptError, ScriptEvent, ScriptHost,
    ScriptInstance, ScriptLanguage, ScriptSource, ScriptStateSnapshot, ScriptValue,
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
