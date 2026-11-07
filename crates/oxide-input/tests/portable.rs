use oxide_input::{JitterReport, KeyEvent, KeyState, SECONDARY_CODES};

#[test]
fn secondary_codes_are_the_exact_approved_linux_codes() {
    assert_eq!(SECONDARY_CODES, [17, 30, 31, 32, 103, 105, 106, 108]);
}

#[test]
fn secondary_codes_map_one_to_one_to_default_solo_note_bindings() {
    // (evdev code, Linux key name, original default Solo binding)
    let expected = [
        (17, "KEY_W", "NOTE_UP = [W, UP]"),
        (30, "KEY_A", "NOTE_LEFT = [A, LEFT]"),
        (31, "KEY_S", "NOTE_DOWN = [S, DOWN]"),
        (32, "KEY_D", "NOTE_RIGHT = [D, RIGHT]"),
        (103, "KEY_UP", "NOTE_UP = [W, UP]"),
        (105, "KEY_LEFT", "NOTE_LEFT = [A, LEFT]"),
        (106, "KEY_RIGHT", "NOTE_RIGHT = [D, RIGHT]"),
        (108, "KEY_DOWN", "NOTE_DOWN = [S, DOWN]"),
    ];

    for (code, linux_name, binding) in expected {
        assert!(
            SECONDARY_CODES.contains(&code),
            "{linux_name} ({code}) missing from SECONDARY_CODES for {binding}"
        );
        assert_eq!(
            SECONDARY_CODES
                .iter()
                .filter(|candidate| **candidate == code)
                .count(),
            1,
            "{linux_name} ({code}) must appear exactly once"
        );
    }
}

#[test]
fn ui_only_keys_are_absent() {
    // Common FlxKey/UI-only Linux equivalents: Esc, Enter, Space.
    for code in [1u16, 28, 57] {
        assert!(
            !SECONDARY_CODES.contains(&code),
            "UI-only code {code} must not be a gameplay code"
        );
    }
}

#[test]
fn key_event_clone_preserves_all_fields() {
    let event = KeyEvent {
        audio_time_ms: 1234.5,
        kernel_time_us: 1_700_000_000_000_123,
        code: 30,
        state: KeyState::Down,
    };
    let cloned = event.clone();

    assert_eq!(
        cloned.audio_time_ms.to_bits(),
        event.audio_time_ms.to_bits()
    );
    assert_eq!(cloned.kernel_time_us, event.kernel_time_us);
    assert_eq!(cloned.code, event.code);
    assert_eq!(cloned.state, KeyState::Down);
    assert_ne!(KeyState::Down, KeyState::Up);
}

#[test]
fn jitter_report_fields_are_plain_public_values() {
    let report = JitterReport {
        p50_ms: 0.25,
        p99_ms: 2.5,
        samples: 7,
    };
    assert_eq!(report.p50_ms, 0.25);
    assert_eq!(report.p99_ms, 2.5);
    assert_eq!(report.samples, 7);
}
