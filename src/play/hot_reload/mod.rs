mod reloaders;
mod types;
mod watch_builders;
mod worker;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use bevy::prelude::*;

use reloaders::{
    HotReloadQueries, HotReloadRes, reload_all_characters_domain, reload_asset_paths,
    reload_chart_domain, reload_scripts_domain, reload_single_character_domain, reload_skin_domain,
    reload_stage_domain,
};
use types::{DomainDiff, HotReloadBatchEvent, WatchPlan};
use watch_builders::{
    build_audio_watch, build_character_watch, build_skin_watch, build_stage_watch,
    build_watch_plan, find_loaded_char,
};
use worker::{WatcherCommand, run_watcher_loop};

struct WatcherHandle {
    cmd_tx: Option<Sender<WatcherCommand>>,
    evt_rx: Receiver<HotReloadBatchEvent>,
    stop_flag: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<()>>,
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        self.cmd_tx.take();
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Resource, Default)]
pub struct HotReloadState {
    initialized: bool,
    generation: u64,
    plan: WatchPlan,
    watcher: Mutex<Option<WatcherHandle>>,
}

impl HotReloadState {
    fn ensure_worker_started(&self) {
        let Ok(mut guard) = self.watcher.lock() else {
            return;
        };
        if guard.is_some() {
            return;
        }
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);

        match thread::Builder::new()
            .name("oxide-hmr-watcher".into())
            .spawn(move || run_watcher_loop(cmd_rx, evt_tx, stop_worker))
        {
            Ok(handle) => {
                *guard = Some(WatcherHandle {
                    cmd_tx: Some(cmd_tx),
                    evt_rx,
                    stop_flag: stop,
                    worker_handle: Some(handle),
                });
            }
            Err(err) => {
                warn!("[HMR] Could not spawn background watcher thread: {err}");
            }
        }
    }

    fn push_plan_to_worker(&mut self, plan: WatchPlan) {
        self.generation = self.generation.wrapping_add(1);
        self.plan = plan.clone();
        if let Ok(guard) = self.watcher.lock()
            && let Some(watcher) = guard.as_ref()
            && let Some(tx) = &watcher.cmd_tx
        {
            let _ = tx.send(WatcherCommand::SetPlan {
                generation: self.generation,
                plan: Box::new(plan),
            });
        }
    }

    fn poll_latest_event(&self) -> Option<HotReloadBatchEvent> {
        let Ok(guard) = self.watcher.lock() else {
            return None;
        };
        let watcher = guard.as_ref()?;
        let mut latest_evt: Option<HotReloadBatchEvent> = None;
        while let Ok(evt) = watcher.evt_rx.try_recv() {
            if evt.generation == self.generation {
                latest_evt = Some(evt);
            }
        }
        latest_evt
    }
}

pub(crate) fn hot_reload_assets_system(
    mut commands: Commands,
    mut hmr: ResMut<HotReloadState>,
    mut res: HotReloadRes,
    mut q: HotReloadQueries,
) {
    if !hmr.initialized {
        hmr.ensure_worker_started();
        let plan = build_watch_plan(&res);
        hmr.push_plan_to_worker(plan);
        hmr.initialized = true;
        return;
    }

    let Some(evt) = hmr.poll_latest_event() else {
        return;
    };

    if evt.mods_changed {
        let _ = res.mods.reload();
        info!(
            "[HMR] Refreshed mod registry ({} active mods)",
            res.mods.mods.len()
        );
    }

    let mut force_stage = evt.mods_changed;
    let mut force_chars = evt.mods_changed;
    let mut force_skin = evt.mods_changed;
    let mut force_audio = evt.mods_changed;
    let mut reload_scripts = evt.scripts_changed || evt.mods_changed;

    if evt.chart.is_changed() {
        let delta = reload_chart_domain(&mut res);
        force_stage |= delta.stage_changed;
        force_chars |= delta.chars_changed;
        force_skin |= delta.skin_changed;
        force_audio |= delta.audio_changed;
        reload_scripts |= delta.stage_changed || delta.chars_changed;
    }

    if !res.state.is_minimal_mode {
        let stage_diff = if force_stage {
            DomainDiff::Full(Vec::new())
        } else {
            evt.stage
        };
        match stage_diff {
            DomainDiff::Unchanged => {}
            DomainDiff::InPlaceAssets(paths) => {
                reload_asset_paths(&res.asset_server, &paths);
                let _ = reload_stage_domain(&mut commands, &mut res, &mut q);
                info!("[HMR] Reloaded {} stage texture(s)", paths.len());
            }
            DomainDiff::Full(paths) => {
                reload_asset_paths(&res.asset_server, &paths);
                let old_scripts_flag = res.stage_res.as_ref().is_some_and(|s| s.scripts);
                if reload_stage_domain(&mut commands, &mut res, &mut q) {
                    let new_scripts_flag = res.stage_res.as_ref().is_some_and(|s| s.scripts);
                    if old_scripts_flag != new_scripts_flag || new_scripts_flag {
                        reload_scripts = true;
                    }
                }
                let new_stage_watch = build_stage_watch(
                    &res.mods,
                    &res.state.chart.stage_id,
                    res.stage_res.as_deref(),
                );
                let extra_paths =
                    new_stage_watch.collect_changed_or_new_server_paths(&hmr.plan.stage);
                reload_asset_paths(&res.asset_server, &extra_paths);
            }
        }

        if force_chars {
            let had_char_scripts = res
                .chars_res
                .as_deref()
                .is_some_and(|cr| cr.characters.iter().any(|c| c.data.script));
            reload_all_characters_domain(&mut commands, &mut res, &mut q);
            let has_char_scripts = res
                .chars_res
                .as_deref()
                .is_some_and(|cr| cr.characters.iter().any(|c| c.data.script));
            if had_char_scripts != has_char_scripts || has_char_scripts {
                reload_scripts = true;
            }
        } else {
            for (char_id, char_diff) in evt.chars {
                let old_watch = hmr.plan.chars.get(&char_id).cloned().unwrap_or_default();
                match char_diff {
                    DomainDiff::Unchanged => {}
                    DomainDiff::InPlaceAssets(paths) => {
                        reload_asset_paths(&res.asset_server, &paths);
                        reload_single_character_domain(&mut commands, &mut res, &mut q, &char_id);
                        info!("[HMR] Reloaded image(s) for character '{char_id}'");
                    }
                    DomainDiff::Full(paths) => {
                        reload_asset_paths(&res.asset_server, &paths);
                        let old_char_script =
                            find_loaded_char(res.chars_res.as_deref(), &char_id)
                                .is_some_and(|c| c.data.script);
                        reload_single_character_domain(&mut commands, &mut res, &mut q, &char_id);
                        let loaded = find_loaded_char(res.chars_res.as_deref(), &char_id);
                        let new_char_script = loaded.is_some_and(|c| c.data.script);
                        if old_char_script != new_char_script || new_char_script {
                            reload_scripts = true;
                        }
                        let new_watch = build_character_watch(&res.mods, &char_id, loaded);
                        let extra_paths =
                            new_watch.collect_changed_or_new_server_paths(&old_watch);
                        reload_asset_paths(&res.asset_server, &extra_paths);
                    }
                }
            }
        }
    }

    let skin_diff = if force_skin {
        DomainDiff::Full(Vec::new())
    } else {
        evt.skin
    };
    match skin_diff {
        DomainDiff::Unchanged => {}
        DomainDiff::InPlaceAssets(paths) => {
            reload_asset_paths(&res.asset_server, &paths);
            info!("[HMR] Reloaded {} skin asset(s)", paths.len());
        }
        DomainDiff::Full(paths) => {
            reload_asset_paths(&res.asset_server, &paths);
            reload_skin_domain(&mut commands, &mut res, &mut q);
            let new_skin_watch =
                build_skin_watch(&res.mods, &res.state.player_strumline.note_style);
            let extra_paths = new_skin_watch.collect_changed_or_new_server_paths(&hmr.plan.skin);
            reload_asset_paths(&res.asset_server, &extra_paths);
        }
    }

    let audio_diff = if force_audio {
        DomainDiff::Full(Vec::new())
    } else {
        evt.audio
    };
    match audio_diff {
        DomainDiff::Unchanged => {}
        DomainDiff::InPlaceAssets(paths) | DomainDiff::Full(paths) => {
            if !paths.is_empty() {
                reload_asset_paths(&res.asset_server, &paths);
                info!("[HMR] Reloaded {} audio track(s)", paths.len());
            }
            let new_audio_watch = build_audio_watch(&res.mods, &res.state);
            let extra_paths =
                new_audio_watch.collect_changed_or_new_server_paths(&hmr.plan.audio);
            reload_asset_paths(&res.asset_server, &extra_paths);
        }
    }

    if reload_scripts {
        reload_scripts_domain(&mut commands, &mut res, &q);
    }

    let refreshed_plan = build_watch_plan(&res);
    hmr.push_plan_to_worker(refreshed_plan);
}
