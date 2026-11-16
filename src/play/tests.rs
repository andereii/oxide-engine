use super::preload::{PlayPreloadTracker, PreloadProgress};

#[test]
fn tracker_exposes_default_progress() {
    let tracker = PlayPreloadTracker::default();
    assert!(!tracker.progress().is_complete());
    assert_eq!(tracker.progress().fraction(), 0.0);
}

#[test]
fn preload_progress_fraction_and_completion_require_both_assets_and_audio() {
    let empty_waiting = PreloadProgress {
        loaded_assets: 0,
        total_assets: 0,
        audio_ready: false,
    };
    assert!(!empty_waiting.is_complete());
    assert_eq!(empty_waiting.fraction(), 0.0);

    let total_assets = 3_usize;
    let loaded_assets = total_assets;
    let partial = PreloadProgress {
        loaded_assets,
        total_assets,
        audio_ready: false,
    };
    let expected_partial = loaded_assets as f32 / (total_assets + 1) as f32;
    assert!(!partial.is_complete());
    assert!((partial.fraction() - expected_partial).abs() < 1e-6);

    let done = PreloadProgress {
        loaded_assets,
        total_assets,
        audio_ready: true,
    };
    assert!(done.is_complete());
    assert_eq!(done.fraction(), 1.0);
}

#[test]
fn composite_piece_z_stays_strictly_monotonic_within_f32_layer_band() {
    use super::stage::schema::{composite_piece_z, stage_layer_z};

    for layer in -2..=8 {
        let base_z = stage_layer_z(layer, 0);
        let next_layer_z = stage_layer_z(layer + 1, 0);
        let mut prev = base_z;
        for piece_idx in 1..=256 {
            let cur = composite_piece_z(base_z, piece_idx);
            assert!(
                cur > prev && cur < next_layer_z,
                "z collapsed or overflowed layer={layer} piece={piece_idx}: prev={prev} cur={cur} next={next_layer_z}"
            );
            prev = cur;
        }
    }
}
