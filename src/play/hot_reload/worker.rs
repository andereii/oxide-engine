use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use super::types::{DomainDiff, FileStamp, HotReloadBatchEvent, ScanCache, WatchPlan};

const HMR_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(super) enum WatcherCommand {
    SetPlan { generation: u64, plan: Box<WatchPlan> },
}

/// Runs on a dedicated background thread so NTFS/filesystem polling never stalls gameplay frames.
pub(super) fn run_watcher_loop(
    cmd_rx: Receiver<WatcherCommand>,
    evt_tx: Sender<HotReloadBatchEvent>,
    stop_flag: Arc<AtomicBool>,
) {
    let mut current_plan: Option<(u64, Box<WatchPlan>)> = None;
    let mut awaiting_ack = false;

    while !stop_flag.load(Ordering::Acquire) {
        match cmd_rx.recv_timeout(HMR_POLL_INTERVAL) {
            Ok(WatcherCommand::SetPlan { generation, mut plan }) => {
                plan.populate_parent_dirs();
                current_plan = Some((generation, plan));
                awaiting_ack = false;
                // Drain queued newer plans in case main sent back-to-back updates.
                while let Ok(WatcherCommand::SetPlan { generation, mut plan }) = cmd_rx.try_recv() {
                    plan.populate_parent_dirs();
                    current_plan = Some((generation, plan));
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }

        if stop_flag.load(Ordering::Acquire) {
            break;
        }
        if awaiting_ack {
            continue;
        }
        let Some((generation, ref plan)) = current_plan else {
            continue;
        };

        let event = scan_plan_once(generation, plan);
        if event.any_change() {
            awaiting_ack = true;
            if evt_tx.send(event).is_err() {
                break;
            }
        }
    }
}

fn scan_plan_once(generation: u64, plan: &WatchPlan) -> HotReloadBatchEvent {
    let mut cache = ScanCache::default();

    let mut mods_changed = plan
        .mods_dir
        .as_ref()
        .is_some_and(|(dir, old)| cache.dir_stamp(dir) != old);
    if !mods_changed {
        mods_changed = plan
            .mod_manifests
            .iter()
            .any(|(path, old)| &FileStamp::from_path(path) != old);
    }

    let chart = plan.chart.diff_with_cache(&mut cache, &plan.parent_dirs);
    let stage = plan.stage.diff_with_cache(&mut cache, &plan.parent_dirs);
    let mut chars: BTreeMap<String, DomainDiff> = BTreeMap::new();
    for (id, watch) in &plan.chars {
        let diff = watch.diff_with_cache(&mut cache, &plan.parent_dirs);
        if diff.is_changed() {
            chars.insert(id.clone(), diff);
        }
    }
    let skin = plan.skin.diff_with_cache(&mut cache, &plan.parent_dirs);
    let scripts_changed = plan
        .scripts
        .diff_with_cache(&mut cache, &plan.parent_dirs)
        .is_changed();
    let audio = plan.audio.diff_with_cache(&mut cache, &plan.parent_dirs);

    HotReloadBatchEvent {
        generation,
        mods_changed,
        chart,
        stage,
        chars,
        skin,
        scripts_changed,
        audio,
    }
}
