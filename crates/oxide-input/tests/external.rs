#![cfg(target_os = "linux")]

//! External tests. They are `#[ignore]`d so a portable `cargo test` never fails
//! on a machine without real input devices or `/dev/uinput` ACL access.
//!
//! Run locally with:
//! `python3 tools/workflow.py run --timeout 300 -- cargo test -p oxide-input -- --include-ignored`

use oxide_input::{InputError, InputStream, probe};

#[test]
#[ignore = "requires real /dev/input (group input or a device ACL)"]
fn real_device_without_input_group_reports_distinct_permission_error() {
    const PATH: &str = "/dev/input/event0";

    assert!(
        std::path::Path::new(PATH).exists(),
        "expected a real evdev node at {PATH}"
    );

    match InputStream::open(PATH) {
        Ok(_) => panic!(
            "expected PermissionDenied for {PATH}; this process unexpectedly has input-group/ACL access"
        ),
        Err(InputError::PermissionDenied { path, .. }) => {
            assert_eq!(path, PATH);
        }
        Err(other) => panic!("expected PermissionDenied, got {other:?}"),
    }
}

#[test]
#[ignore = "requires /dev/uinput ACL (this machine has it; CI runners may not)"]
fn uinput_virtual_device_probe_round_trip() {
    match InputStream::open_virtual() {
        Ok(mut stream) => {
            let events: Vec<_> = stream.poll(0.0).collect();
            assert!(events.is_empty(), "fresh virtual device yielded events");
            assert!(
                stream.take_error().is_none(),
                "fresh virtual device stored a read error"
            );
        }
        Err(error) => panic!("open_virtual failed: {error:?}"),
    }

    let report = match probe::run(8) {
        Ok(report) => report,
        Err(error) => panic!("probe::run failed: {error:?}"),
    };
    assert!(report.samples > 0, "probe observed no samples");
    assert!(report.samples <= 8, "probe observed more than emitted");
    assert!(report.p50_ms.is_finite(), "p50 is not finite");
    assert!(report.p99_ms.is_finite(), "p99 is not finite");
    assert!(
        report.p50_ms <= report.p99_ms,
        "nearest-rank percentiles must be ordered"
    );
}
