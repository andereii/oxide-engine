use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::Resource;

use crate::mods::{DiscoveredScript, ModRegistry, ModStore};

use super::api::{
    LogLevel, MAX_CUSTOM_HOOK_DEPTH, PropertyTween, ScriptCommand, ScriptContext,
    ScriptStateSnapshot, ScriptTimer, TweenProp, TweenTarget,
};
use super::backend::{RhaiBackend, ScriptBackend, ScriptError, ScriptInstance, ScriptSource};
use super::diagnostics::{ScriptDiagKind, ScriptDiagnostic};
use super::events::{HookKind, ScriptEvent};
use super::value::{ScriptLanguage, ScriptValue};

#[derive(Resource)]
pub struct ScriptHost {
    backends: HashMap<ScriptLanguage, Box<dyn ScriptBackend>>,
    instances: Vec<Box<dyn ScriptInstance>>,
    tweens: Vec<PropertyTween>,
    timers: Vec<ScriptTimer>,
    next_timer_id: u64,
    rng_state: u64,
    cached_registry: Option<(u64, usize, Arc<ModRegistry>)>,
    pub compile_errors: Vec<String>,
    pub runtime_errors: Vec<String>,
    pub diagnostics: Vec<ScriptDiagnostic>,
    pub log_epoch: u64,
    next_diag_seq: u64,
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::with_default_backends()
    }
}

impl ScriptHost {
    pub fn empty() -> Self {
        Self {
            backends: HashMap::new(),
            instances: Vec::new(),
            tweens: Vec::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            rng_state: 0x517C_C1B7_2722_0A95,
            cached_registry: None,
            compile_errors: Vec::new(),
            runtime_errors: Vec::new(),
            diagnostics: Vec::new(),
            log_epoch: 0,
            next_diag_seq: 0,
        }
    }

    pub fn with_default_backends() -> Self {
        let mut host = Self::empty();
        host.register_backend(Box::new(RhaiBackend::new()));
        host
    }

    pub fn with_registry(mods: ModRegistry) -> Self {
        let mut host = Self::empty();
        host.register_backend(Box::new(RhaiBackend::with_registry(mods)));
        host
    }

    pub fn register_backend(&mut self, backend: Box<dyn ScriptBackend>) {
        self.backends.insert(backend.language(), backend);
    }

    pub fn supported_extensions(&self) -> Vec<String> {
        let mut exts = Vec::new();
        for backend in self.backends.values() {
            for &ext in backend.extensions() {
                exts.push(ext.to_string());
            }
        }
        exts.sort();
        exts.dedup();
        exts
    }

    pub fn load_source(&mut self, ext: &str, source: ScriptSource) -> Result<(), ScriptError> {
        let Some(backend) = self
            .backends
            .values()
            .find(|b| b.supports_extension(ext))
        else {
            return Err(ScriptError::UnsupportedExtension(ext.to_string()));
        };
        let instance = backend.compile(source)?;
        self.instances.push(instance);
        self.sort_instances();
        Ok(())
    }

    pub fn load_discovered(&mut self, scripts: &[DiscoveredScript]) {
        self.clear_scripts();
        for disc in scripts {
            if !self.backends.values().any(|b| b.supports_extension(&disc.extension)) {
                continue;
            }
            match ScriptSource::from_discovered(disc) {
                Ok(source) => {
                    if let Err(err) = self.load_source(&disc.extension, source) {
                        self.note_script_failure(&err, false);
                    }
                }
                Err(err) => {
                    self.note_script_failure(&err, false);
                }
            }
        }
    }

    pub fn clear_scripts(&mut self) {
        self.instances.clear();
        self.tweens.clear();
        self.timers.clear();
        self.compile_errors.clear();
        self.runtime_errors.clear();
        self.diagnostics.clear();
        self.next_diag_seq = 0;
        self.log_epoch = self.log_epoch.wrapping_add(1);
        self.cached_registry = None;
        for backend in self.backends.values() {
            backend.invalidate_caches();
        }
    }

    fn note_script_failure(&mut self, err: &ScriptError, runtime: bool) -> Option<String> {
        if !self.push_diagnostic(ScriptDiagnostic::from_error(err)) {
            return None;
        }
        let text = err.to_string();
        if runtime {
            push_capped(&mut self.runtime_errors, text.clone());
        } else {
            push_capped(&mut self.compile_errors, text.clone());
        }
        Some(text)
    }

    fn note_runtime_message(&mut self, file: &str, hook: Option<&str>, message: String) -> bool {
        let fresh = self.push_diagnostic(ScriptDiagnostic {
            kind: ScriptDiagKind::Runtime,
            file: file.to_string(),
            line: None,
            column: None,
            hook: hook.map(str::to_string),
            message: message.clone(),
            repeats: 1,
            seq: 0,
        });
        if fresh {
            push_capped(&mut self.runtime_errors, message);
        }
        fresh
    }

    fn push_diagnostic(&mut self, mut diag: ScriptDiagnostic) -> bool {
        if let Some(existing) = self
            .diagnostics
            .iter_mut()
            .rev()
            .find(|prev| prev.same_error(&diag))
        {
            existing.repeats = existing.repeats.saturating_add(1);
            return false;
        }
        self.next_diag_seq = self.next_diag_seq.saturating_add(1);
        diag.seq = self.next_diag_seq;
        diag.repeats = 1;
        self.diagnostics.push(diag);
        const MAX_SCRIPT_DIAGNOSTICS: usize = 64;
        if self.diagnostics.len() > MAX_SCRIPT_DIAGNOSTICS {
            let overflow = self.diagnostics.len() - MAX_SCRIPT_DIAGNOSTICS;
            self.diagnostics.drain(0..overflow);
        }
        true
    }

    pub fn script_count(&self) -> usize {
        self.instances.len()
    }

    /// Returns `true` if any loaded script instance defines a handler for `hook`.
    pub fn has_hook_listeners(&self, hook: &HookKind) -> bool {
        self.instances.iter().any(|inst| inst.has_hook(hook))
    }

    /// Returns `true` if any active tweens, timers, or `Update` hook listeners need per-frame work.
    pub fn has_frame_update_work(&self) -> bool {
        !self.tweens.is_empty()
            || !self.timers.is_empty()
            || self.has_hook_listeners(&HookKind::Update)
    }

    pub fn script_ids(&self) -> Vec<&str> {
        self.instances.iter().map(|s| s.id()).collect()
    }

    pub fn watched_script_paths(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .instances
            .iter()
            .filter_map(|inst| inst.path().map(PathBuf::from))
            .collect();
        for backend in self.backends.values() {
            out.extend(backend.watched_module_paths());
        }
        out
    }

    fn sort_instances(&mut self) {
        self.instances.sort_by(|a, b| {
            a.scope()
                .order_rank()
                .cmp(&b.scope().order_rank())
                .then_with(|| a.priority().cmp(&b.priority()))
                .then_with(|| a.mod_id().cmp(b.mod_id()))
                .then_with(|| a.id().cmp(b.id()))
        });
    }

    pub fn dispatch(
        &mut self,
        hook: &HookKind,
        event: &mut ScriptEvent,
        ctx: &mut ScriptContext,
    ) {
        self.dispatch_at_depth(hook, event, ctx, 0);
    }

    fn dispatch_at_depth(
        &mut self,
        hook: &HookKind,
        event: &mut ScriptEvent,
        ctx: &mut ScriptContext,
        depth: usize,
    ) {
        let is_create = matches!(hook, HookKind::Create);
        let mut failures = Vec::new();
        for inst in &mut self.instances {
            if event.propagation_stopped {
                break;
            }
            if !is_create && !inst.has_hook(hook) {
                continue;
            }
            if let Err(err) = inst.call_hook(hook, event, ctx) {
                failures.push((inst.mod_id().to_string(), err));
            }
        }
        for (mod_id, err) in failures {
            let Some(message) = self.note_script_failure(&err, true) else {
                continue;
            };
            ctx.commands.push(ScriptCommand::Log {
                level: LogLevel::Error,
                mod_id,
                message,
            });
        }
        self.absorb_context_requests(ctx, depth);
    }

    pub fn tick_timers_and_tweens(&mut self, dt_s: f64, ctx: &mut ScriptContext) {
        let dt = dt_s.max(0.0);

        // Advance active property tweens and emit corresponding updates.
        let mut finished_cbs: Vec<(String, String)> = Vec::new();
        for tween in &mut self.tweens {
            tween.elapsed_s += dt;
            let val = tween.sample();
            emit_tween_value(&tween.target, tween.prop, val, ctx);
            if tween.is_finished()
                && let Some(cb) = &tween.on_complete
            {
                finished_cbs.push(cb.clone());
            }
        }
        self.tweens.retain(|t| !t.is_finished());

        // Advance timers and fire ready callbacks.
        let mut ready_calls: Vec<(String, String, Vec<ScriptValue>)> = Vec::new();
        for timer in &mut self.timers {
            timer.remaining_s -= dt;
            if timer.remaining_s <= 0.0 {
                ready_calls.push((timer.script_id.clone(), timer.fn_name.clone(), timer.args.clone()));
                if let Some(left) = &mut timer.repeats_left {
                    *left = left.saturating_sub(1);
                }
                timer.remaining_s += timer.interval_s;
            }
        }
        self.timers
            .retain(|t| t.repeats_left.is_none_or(|left| left > 0));

        for (script_id, fn_name) in finished_cbs {
            self.call_fn_on_script(&script_id, &fn_name, &[], ctx);
        }
        for (script_id, fn_name, args) in ready_calls {
            self.call_fn_on_script(&script_id, &fn_name, &args, ctx);
        }
        self.absorb_context_requests(ctx, 0);
    }

    pub fn call_fn_on_script(
        &mut self,
        script_id: &str,
        fn_name: &str,
        args: &[ScriptValue],
        ctx: &mut ScriptContext,
    ) -> Option<ScriptValue> {
        let found = self.instances.iter().position(|inst| inst.id() == script_id)?;
        let call_res = {
            let inst = &mut self.instances[found];
            let mod_id = inst.mod_id().to_string();
            match inst.call_fn(fn_name, args, ctx) {
                Ok(v) => Ok(v),
                Err(err) => Err((mod_id, err)),
            }
        };
        let out = match call_res {
            Ok(v) => Some(v),
            Err((mod_id, err)) => {
                if let Some(message) = self.note_script_failure(&err, true) {
                    ctx.commands.push(ScriptCommand::Log {
                        level: LogLevel::Error,
                        mod_id,
                        message,
                    });
                }
                None
            }
        };
        self.absorb_context_requests(ctx, 0);
        out
    }

    fn get_or_cache_registry(&mut self, registry: &ModRegistry) -> Arc<ModRegistry> {
        let key = (registry.reload_count, registry.mods.len());
        if let Some((reload_count, mods_len, cached)) = &self.cached_registry
            && *reload_count == key.0
            && *mods_len == key.1
            && cached.assets_dir == registry.assets_dir
            && cached.mods_dir == registry.mods_dir
        {
            return Arc::clone(cached);
        }
        let arc = Arc::new(registry.clone());
        self.cached_registry = Some((key.0, key.1, Arc::clone(&arc)));
        arc
    }

    pub fn make_context(
        &mut self,
        snapshot: ScriptStateSnapshot,
        registry: &ModRegistry,
        store: &mut ModStore,
    ) -> ScriptContext {
        let reg_arc = self.get_or_cache_registry(registry);
        ScriptContext::new(snapshot, reg_arc, std::mem::take(store), self.rng_state)
    }

    pub fn finalize_context(&mut self, mut ctx: ScriptContext, store: &mut ModStore) -> ScriptContext {
        self.rng_state = ctx.rng_state;
        *store = std::mem::take(&mut ctx.store);
        ctx
    }

    fn absorb_context_requests(&mut self, ctx: &mut ScriptContext, depth: usize) {
        if !ctx.cancel_tween_targets.is_empty() {
            let targets = std::mem::take(&mut ctx.cancel_tween_targets);
            self.tweens.retain(|t| !targets.contains(&t.target));
        }
        if !ctx.cancel_tween_props.is_empty() {
            let props = std::mem::take(&mut ctx.cancel_tween_props);
            self.tweens
                .retain(|t| !props.iter().any(|(tgt, p)| *tgt == t.target && *p == t.prop));
        }

        let drained: Vec<PropertyTween> = ctx.new_tweens.drain(..).collect();
        let mut instant_cbs: Vec<(String, String)> = Vec::new();
        for new_tween in drained {
            if new_tween.duration_s <= 0.0 {
                emit_tween_value(&new_tween.target, new_tween.prop, new_tween.end, ctx);
                if let Some(cb) = new_tween.on_complete {
                    instant_cbs.push(cb);
                }
                continue;
            }
            self.tweens
                .retain(|t| !(t.target == new_tween.target && t.prop == new_tween.prop));
            self.tweens.push(new_tween);
        }

        for (sid, fn_name) in ctx.cancel_timers_for_fn.drain(..) {
            self.timers
                .retain(|t| !(t.script_id == sid && t.fn_name == fn_name));
        }

        for req in ctx.new_timers.drain(..) {
            let id = self.next_timer_id;
            self.next_timer_id += 1;
            self.timers.push(ScriptTimer {
                id,
                script_id: req.script_id,
                fn_name: req.fn_name,
                interval_s: req.interval_s,
                remaining_s: req.interval_s,
                repeats_left: req.repeats_left,
                args: req.args,
            });
        }

        for (script_id, fn_name) in instant_cbs {
            self.call_fn_on_script(&script_id, &fn_name, &[], ctx);
        }

        self.flush_pending_custom_hooks(ctx, depth);
    }

    fn flush_pending_custom_hooks(&mut self, ctx: &mut ScriptContext, depth: usize) {
        let queued = std::mem::take(&mut ctx.pending_custom_hooks);
        if queued.is_empty() {
            return;
        }
        // Bound nested custom-hook cascades so cyclic/self-triggering `emit(...)` calls cannot hang.
        if depth >= MAX_CUSTOM_HOOK_DEPTH {
            for req in queued {
                let msg = format!(
                    "custom hook '{}' exceeded max dispatch depth ({MAX_CUSTOM_HOOK_DEPTH})",
                    req.name
                );
                if self.note_runtime_message(&req.sender_script_id, Some(req.name.as_str()), msg.clone())
                {
                    ctx.commands.push(ScriptCommand::Log {
                        level: LogLevel::Error,
                        mod_id: req.sender_mod_id,
                        message: msg,
                    });
                }
            }
            return;
        }
        for req in queued {
            let hook_name = req.name.trim();
            if hook_name.is_empty() {
                continue;
            }
            let hook = HookKind::Custom(hook_name.to_string());
            let mut ev = ScriptEvent::new(hook.clone())
                .with_field("name", hook_name)
                .with_field("value", req.data.clone())
                .with_field("data", req.data)
                .with_field("source", req.sender_script_id);
            self.dispatch_at_depth(&hook, &mut ev, ctx, depth + 1);
        }
    }
}

fn push_capped(log: &mut Vec<String>, text: String) {
    const MAX_LOG_LINES: usize = 64;
    if log.len() >= MAX_LOG_LINES {
        return;
    }
    log.push(text);
}

fn emit_tween_value(target: &TweenTarget, prop: TweenProp, val: f64, ctx: &mut ScriptContext) {
    match (target, prop) {
        (TweenTarget::Camera, TweenProp::X) => {
            ctx.mutable.cam_x = val;
            ctx.mutable.dirty_cam_pos = true;
        }
        (TweenTarget::Camera, TweenProp::Y) => {
            ctx.mutable.cam_y = val;
            ctx.mutable.dirty_cam_pos = true;
        }
        (TweenTarget::Camera, TweenProp::Zoom) => {
            ctx.mutable.cam_zoom = val;
            ctx.mutable.dirty_cam_zoom = true;
        }
        (TweenTarget::Camera, TweenProp::HudZoom) => {
            ctx.mutable.hud_zoom = val;
            ctx.mutable.dirty_hud_zoom = true;
        }
        (TweenTarget::Camera, TweenProp::Angle) => {
            ctx.mutable.cam_angle_deg = val as f32;
            ctx.mutable.dirty_cam_angle = true;
        }
        (TweenTarget::Camera, TweenProp::HudAngle) => {
            ctx.mutable.hud_angle_deg = val as f32;
            ctx.mutable.dirty_hud_angle = true;
        }
        (TweenTarget::Scroll, TweenProp::Scroll) => {
            ctx.mutable.scroll_speed = val;
            ctx.mutable.dirty_scroll_speed = true;
        }
        (TweenTarget::Lane(lane), TweenProp::X) => {
            let cur_y = ctx.snapshot.lane_offsets.get(lane).map_or(0.0, |p| p[1]);
            let x = val as f32;
            ctx.snapshot.lane_offsets.insert(*lane, [x, cur_y]);
            ctx.commands.push(ScriptCommand::SetLaneOffset { lane: *lane, x, y: cur_y });
        }
        (TweenTarget::Lane(lane), TweenProp::Y) => {
            let cur_x = ctx.snapshot.lane_offsets.get(lane).map_or(0.0, |p| p[0]);
            let y = val as f32;
            ctx.snapshot.lane_offsets.insert(*lane, [cur_x, y]);
            ctx.commands.push(ScriptCommand::SetLaneOffset { lane: *lane, x: cur_x, y });
        }
        (TweenTarget::Lane(lane), TweenProp::Angle) => {
            let deg = val as f32;
            ctx.snapshot.lane_angles.insert(*lane, deg);
            ctx.commands.push(ScriptCommand::SetLaneAngle { lane: *lane, deg });
        }
        (TweenTarget::Lane(lane), TweenProp::ScaleX) => {
            let cur_sy = ctx.snapshot.lane_scales.get(lane).map_or(1.0, |s| s[1]);
            let sx = val as f32;
            ctx.snapshot.lane_scales.insert(*lane, [sx, cur_sy]);
            ctx.commands.push(ScriptCommand::SetLaneScale { lane: *lane, sx, sy: cur_sy });
        }
        (TweenTarget::Lane(lane), TweenProp::ScaleY) => {
            let cur_sx = ctx.snapshot.lane_scales.get(lane).map_or(1.0, |s| s[0]);
            let sy = val as f32;
            ctx.snapshot.lane_scales.insert(*lane, [cur_sx, sy]);
            ctx.commands.push(ScriptCommand::SetLaneScale { lane: *lane, sx: cur_sx, sy });
        }
        (TweenTarget::Lane(lane), TweenProp::Alpha) => {
            let alpha = (val as f32).clamp(0.0, 1.0);
            ctx.snapshot.lane_alphas.insert(*lane, alpha);
            ctx.commands.push(ScriptCommand::SetLaneAlpha { lane: *lane, alpha });
        }
        (TweenTarget::Prop(id), prop) => {
            let v = val as f32;
            match prop {
                TweenProp::X => {
                    let y = ctx.snapshot.prop_positions.get(id).map_or(0.0, |p| p[1]);
                    ctx.snapshot.prop_positions.insert(id.clone(), [v, y]);
                }
                TweenProp::Y => {
                    let x = ctx.snapshot.prop_positions.get(id).map_or(0.0, |p| p[0]);
                    ctx.snapshot.prop_positions.insert(id.clone(), [x, v]);
                }
                TweenProp::ScaleX => {
                    let sy = ctx.snapshot.prop_scales.get(id).map_or(1.0, |s| s[1]);
                    ctx.snapshot.prop_scales.insert(id.clone(), [v, sy]);
                }
                TweenProp::ScaleY => {
                    let sx = ctx.snapshot.prop_scales.get(id).map_or(1.0, |s| s[0]);
                    ctx.snapshot.prop_scales.insert(id.clone(), [sx, v]);
                }
                TweenProp::Angle => {
                    ctx.snapshot.prop_angles.insert(id.clone(), v);
                }
                TweenProp::Alpha => {
                    ctx.snapshot.prop_alphas.insert(id.clone(), v.clamp(0.0, 1.0));
                }
                TweenProp::Z => {
                    ctx.snapshot.prop_z.insert(id.clone(), v);
                }
                TweenProp::VelX => {
                    let vy = ctx.snapshot.prop_velocities.get(id).map_or(0.0, |vel| vel[1]);
                    ctx.snapshot.prop_velocities.insert(id.clone(), [v, vy]);
                }
                TweenProp::VelY => {
                    let vx = ctx.snapshot.prop_velocities.get(id).map_or(0.0, |vel| vel[0]);
                    ctx.snapshot.prop_velocities.insert(id.clone(), [vx, v]);
                }
                _ => {}
            }
            ctx.commands.push(ScriptCommand::SetPropNumber {
                id: id.clone(),
                prop,
                value: v,
            });
        }
        (TweenTarget::Char(id), prop) => {
            let v = val as f32;
            match prop {
                TweenProp::X => {
                    let y = ctx.snapshot.char_positions.get(id).map_or(0.0, |p| p[1]);
                    ctx.snapshot.char_positions.insert(id.clone(), [v, y]);
                    ctx.commands.push(ScriptCommand::SetCharacterPos { target: id.clone(), x: v, y });
                }
                TweenProp::Y => {
                    let x = ctx.snapshot.char_positions.get(id).map_or(0.0, |p| p[0]);
                    ctx.snapshot.char_positions.insert(id.clone(), [x, v]);
                    ctx.commands.push(ScriptCommand::SetCharacterPos { target: id.clone(), x, y: v });
                }
                TweenProp::ScaleX => {
                    let sy = ctx.snapshot.char_scales.get(id).map_or(1.0, |s| s[1]);
                    ctx.snapshot.char_scales.insert(id.clone(), [v, sy]);
                    ctx.commands.push(ScriptCommand::SetCharacterScale { target: id.clone(), sx: v, sy });
                }
                TweenProp::ScaleY => {
                    let sx = ctx.snapshot.char_scales.get(id).map_or(1.0, |s| s[0]);
                    ctx.snapshot.char_scales.insert(id.clone(), [sx, v]);
                    ctx.commands.push(ScriptCommand::SetCharacterScale { target: id.clone(), sx, sy: v });
                }
                TweenProp::Alpha => {
                    let a = v.clamp(0.0, 1.0);
                    ctx.snapshot.char_alphas.insert(id.clone(), a);
                    ctx.commands.push(ScriptCommand::SetCharacterAlpha { target: id.clone(), alpha: a });
                }
                TweenProp::Z => {
                    ctx.snapshot.char_z.insert(id.clone(), v);
                    ctx.commands.push(ScriptCommand::SetCharZ { target: id.clone(), z: v });
                }
                _ => {}
            }
        }
        (TweenTarget::Sound(tag), TweenProp::Volume) => {
            let v = (val as f32).clamp(0.0, 2.0);
            ctx.snapshot.sound_volumes.insert(tag.clone(), v);
            ctx.commands.push(ScriptCommand::SetSoundParam {
                tag: tag.clone(),
                volume: Some(v),
                pitch: None,
            });
        }
        (TweenTarget::Text(id), prop) => {
            let v = val as f32;
            match prop {
                TweenProp::X => {
                    let y = ctx.snapshot.text_positions.get(id).map_or(0.0, |p| p[1]);
                    ctx.snapshot.text_positions.insert(id.clone(), [v, y]);
                }
                TweenProp::Y => {
                    let x = ctx.snapshot.text_positions.get(id).map_or(0.0, |p| p[0]);
                    ctx.snapshot.text_positions.insert(id.clone(), [x, v]);
                }
                TweenProp::Alpha => {
                    ctx.snapshot.text_alphas.insert(id.clone(), v.clamp(0.0, 1.0));
                }
                _ => {}
            }
            ctx.commands.push(ScriptCommand::SetPropNumber {
                id: id.clone(),
                prop,
                value: v,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::ScriptHost;
    use super::super::api::ScriptStateSnapshot;
    use super::super::backend::ScriptSource;
    use super::super::diagnostics::ScriptDiagKind;
    use super::super::events::{HookKind, ScriptEvent};
    use crate::mods::{DiscoveredScript, ModRegistry, ModStore, ScriptScope};

    #[test]
    fn script_failures_keep_file_and_line() {
        let dir = std::env::temp_dir().join(format!("oxide-script-err-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.rhai");
        fs::write(&path, "fn onCreate() {\n    let x = ;\n}\n").unwrap();

        let mods = ModRegistry::discover(dir.clone(), dir.join("mods-missing"));
        let mut host = ScriptHost::with_registry(mods.clone());
        let discovered = DiscoveredScript {
            id: "demo:bad.rhai".into(),
            mod_id: "demo".into(),
            name: "bad".into(),
            extension: "rhai".into(),
            rel_path: "bad.rhai".into(),
            path,
            scope: ScriptScope::Global,
            priority: 0,
        };
        host.load_discovered(std::slice::from_ref(&discovered));
        assert_eq!(host.diagnostics.len(), 1, "{:?}", host.compile_errors);
        assert_eq!(host.diagnostics[0].kind, ScriptDiagKind::Compile);
        assert_eq!(host.diagnostics[0].file, "demo/bad.rhai");
        assert_eq!(host.diagnostics[0].line, Some(2));

        let source = ScriptSource::inline(
            "runtime.rhai",
            "demo",
            "fn onCreate() {\n    missing;\n}\n",
        );
        host.load_source("rhai", source).unwrap();
        let mut store = ModStore::with_root(&dir);
        let mut ctx = host.make_context(ScriptStateSnapshot::empty_placeholder(), &mods, &mut store);
        let mut event = ScriptEvent::new(HookKind::Create);
        host.dispatch(&HookKind::Create, &mut event, &mut ctx);
        host.dispatch(&HookKind::Create, &mut event, &mut ctx);

        let runtime: Vec<_> = host
            .diagnostics
            .iter()
            .filter(|diag| diag.kind == ScriptDiagKind::Runtime)
            .collect();
        assert_eq!(runtime.len(), 1, "{:?}", host.runtime_errors);
        assert_eq!(runtime[0].file, "runtime.rhai");
        assert_eq!(runtime[0].line, Some(2));
        assert!(runtime[0].repeats >= 2, "repeats {}", runtime[0].repeats);
        assert_eq!(host.runtime_errors.len(), 1, "{:?}", host.runtime_errors);
        assert!(
            runtime[0].message.to_lowercase().contains("not found"),
            "{}",
            runtime[0].message
        );

        let import = ScriptSource::inline(
            "import-miss.rhai",
            "demo",
            "fn onCreate() {\n    import \"./MissingWeekendModule\" as missing_mod;\n}\n",
        );
        host.load_source("rhai", import).unwrap();
        let mut event = ScriptEvent::new(HookKind::Create);
        host.dispatch(&HookKind::Create, &mut event, &mut ctx);
        let before = host.runtime_errors.len();
        host.dispatch(&HookKind::Create, &mut event, &mut ctx);
        assert_eq!(host.runtime_errors.len(), before);
        let import_err = host
            .diagnostics
            .iter()
            .find(|diag| diag.file == "import-miss.rhai")
            .expect("import miss");
        assert!(import_err.repeats >= 2, "repeats {}", import_err.repeats);
        assert!(
            import_err.message.to_lowercase().contains("module"),
            "{}",
            import_err.message
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn relative_import_resolves_next_to_the_script() {
        let dir = std::env::temp_dir().join(format!(
            "oxide-rel-import-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let nene = dir.join("gameplay/characters/nene");
        fs::create_dir_all(&nene).unwrap();
        fs::write(nene.join("ABotAtlasSprite.rhai"), "fn marker() { 1 }\n").unwrap();
        let script_path = nene.join("nene.rhai");
        let code = "fn onCreate() {\n    import \"./ABotAtlasSprite\" as abot_mod;\n}\n";
        fs::write(&script_path, code).unwrap();

        let mods = ModRegistry::discover(dir.clone(), dir.join("mods-missing"));
        let mut host = ScriptHost::with_registry(mods.clone());
        let source = super::super::backend::ScriptSource {
            id: "assets:gameplay/characters/nene/nene.rhai".into(),
            mod_id: "assets".into(),
            name: "nene".into(),
            scope: ScriptScope::Global,
            priority: 0,
            path: Some(script_path),
            code: code.into(),
        };
        host.load_source("rhai", source).unwrap();
        let mut store = ModStore::with_root(&dir);
        let mut ctx = host.make_context(ScriptStateSnapshot::empty_placeholder(), &mods, &mut store);
        let mut event = ScriptEvent::new(HookKind::Create);
        host.dispatch(&HookKind::Create, &mut event, &mut ctx);
        assert!(
            host.runtime_errors.is_empty(),
            "{:?}",
            host.runtime_errors
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
