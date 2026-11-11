//! flixel 6.1.2 `FlxEase` family, shared by the schema-3 sim and the scene.
//!
//! The formulas are transcribed from `.haxelib/flixel/git/flixel/tweens/
//! FlxEase.hx` (observed source reference, kept as f64 per the M5 policy) and
//! are frozen by `docs/M6-CONTRACT.md` §12.3/§12.6. `slice::events` resolves
//! chart event ease names into these functions; `render::scene` applies the
//! same functions to its follow/zoom/scroll tweens. The bodies stay verbatim
//! in this one module so both consumers use one transcription.

/// A resolved ease is its `FlxEase` field name plus the f64 function.
pub(crate) type EaseFn = fn(f64) -> f64;
pub(crate) type ResolvedEase = (&'static str, EaseFn);

pub(crate) fn resolve_ease(name: &str) -> Option<ResolvedEase> {
    type Ease = fn(f64) -> f64;
    let resolved: (&'static str, Ease) = match name {
        "linear" => ("linear", linear),
        "quadIn" => ("quadIn", quad_in),
        "quadOut" => ("quadOut", quad_out),
        "quadInOut" => ("quadInOut", quad_in_out),
        "cubeIn" => ("cubeIn", cube_in),
        "cubeOut" => ("cubeOut", cube_out),
        "cubeInOut" => ("cubeInOut", cube_in_out),
        "quartIn" => ("quartIn", quart_in),
        "quartOut" => ("quartOut", quart_out),
        "quartInOut" => ("quartInOut", quart_in_out),
        "quintIn" => ("quintIn", quint_in),
        "quintOut" => ("quintOut", quint_out),
        "quintInOut" => ("quintInOut", quint_in_out),
        "sineIn" => ("sineIn", sine_in),
        "sineOut" => ("sineOut", sine_out),
        "sineInOut" => ("sineInOut", sine_in_out),
        "expoIn" => ("expoIn", expo_in),
        "expoOut" => ("expoOut", expo_out),
        "expoInOut" => ("expoInOut", expo_in_out),
        "circIn" => ("circIn", circ_in),
        "circOut" => ("circOut", circ_out),
        "circInOut" => ("circInOut", circ_in_out),
        "backIn" => ("backIn", back_in),
        "backOut" => ("backOut", back_out),
        "backInOut" => ("backInOut", back_in_out),
        "bounceIn" => ("bounceIn", bounce_in),
        "bounceOut" => ("bounceOut", bounce_out),
        "bounceInOut" => ("bounceInOut", bounce_in_out),
        "smoothStepIn" => ("smoothStepIn", smooth_step_in),
        "smoothStepOut" => ("smoothStepOut", smooth_step_out),
        "smoothStepInOut" => ("smoothStepInOut", smooth_step_in_out),
        "smootherStepIn" => ("smootherStepIn", smoother_step_in),
        "smootherStepOut" => ("smootherStepOut", smoother_step_out),
        "smootherStepInOut" => ("smootherStepInOut", smoother_step_in_out),
        "elasticIn" => ("elasticIn", elastic_in),
        "elasticOut" => ("elasticOut", elastic_out),
        "elasticInOut" => ("elasticInOut", elastic_in_out),
        _ => return None,
    };
    Some(resolved)
}

const ELASTIC_AMPLITUDE: f64 = 1.0;
const ELASTIC_PERIOD: f64 = 0.4;
const B1: f64 = 1.0 / 2.75;
const B2: f64 = 2.0 / 2.75;
const B3: f64 = 1.5 / 2.75;
const B4: f64 = 2.5 / 2.75;
const B5: f64 = 2.25 / 2.75;
const B6: f64 = 2.625 / 2.75;

const PI2: f64 = std::f64::consts::PI / 2.0;

/// The identity ease (the flixel `FlxTween` default); pub(crate) for the
/// cutscene step table's fallback.
pub(crate) use self::linear as linear_ease;
pub(crate) fn linear(t: f64) -> f64 {
    t
}

fn quad_in(t: f64) -> f64 {
    t * t
}

fn quad_out(t: f64) -> f64 {
    -t * (t - 2.0)
}

fn quad_in_out(t: f64) -> f64 {
    if t <= 0.5 {
        t * t * 2.0
    } else {
        let t = t - 1.0;
        1.0 - t * t * 2.0
    }
}

fn cube_in(t: f64) -> f64 {
    t * t * t
}

fn cube_out(t: f64) -> f64 {
    let t = t - 1.0;
    1.0 + t * t * t
}

fn cube_in_out(t: f64) -> f64 {
    if t <= 0.5 {
        t * t * t * 4.0
    } else {
        let t = t - 1.0;
        1.0 + t * t * t * 4.0
    }
}

fn quart_in(t: f64) -> f64 {
    t * t * t * t
}

fn quart_out(t: f64) -> f64 {
    let t = t - 1.0;
    1.0 - t * t * t * t
}

fn quart_in_out(t: f64) -> f64 {
    if t <= 0.5 {
        t * t * t * t * 8.0
    } else {
        let t = t * 2.0 - 2.0;
        (1.0 - t * t * t * t) / 2.0 + 0.5
    }
}

fn quint_in(t: f64) -> f64 {
    t * t * t * t * t
}

fn quint_out(t: f64) -> f64 {
    let t = t - 1.0;
    t * t * t * t * t + 1.0
}

fn quint_in_out(t: f64) -> f64 {
    let t = t * 2.0;
    if t < 1.0 {
        (t * t * t * t * t) / 2.0
    } else {
        let t = t - 2.0;
        (t * t * t * t * t + 2.0) / 2.0
    }
}

fn smooth_step_in(t: f64) -> f64 {
    2.0 * smooth_step_in_out(t / 2.0)
}

fn smooth_step_out(t: f64) -> f64 {
    2.0 * smooth_step_in_out(t / 2.0 + 0.5) - 1.0
}

fn smooth_step_in_out(t: f64) -> f64 {
    t * t * (t * -2.0 + 3.0)
}

fn smoother_step_in(t: f64) -> f64 {
    2.0 * smoother_step_in_out(t / 2.0)
}

fn smoother_step_out(t: f64) -> f64 {
    2.0 * smoother_step_in_out(t / 2.0 + 0.5) - 1.0
}

fn smoother_step_in_out(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn sine_in(t: f64) -> f64 {
    -f64::cos(PI2 * t) + 1.0
}

fn sine_out(t: f64) -> f64 {
    f64::sin(PI2 * t)
}

fn sine_in_out(t: f64) -> f64 {
    -f64::cos(std::f64::consts::PI * t) / 2.0 + 0.5
}

fn bounce_in(t: f64) -> f64 {
    1.0 - bounce_out(1.0 - t)
}

fn bounce_out(t: f64) -> f64 {
    if t < B1 {
        7.5625 * t * t
    } else if t < B2 {
        7.5625 * (t - B3) * (t - B3) + 0.75
    } else if t < B4 {
        7.5625 * (t - B5) * (t - B5) + 0.9375
    } else {
        7.5625 * (t - B6) * (t - B6) + 0.984_375
    }
}

fn bounce_in_out(t: f64) -> f64 {
    if t < 0.5 {
        (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
    } else {
        (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
    }
}

fn circ_in(t: f64) -> f64 {
    -((1.0 - t * t).sqrt() - 1.0)
}

fn circ_out(t: f64) -> f64 {
    (1.0 - (t - 1.0) * (t - 1.0)).sqrt()
}

fn circ_in_out(t: f64) -> f64 {
    if t <= 0.5 {
        ((1.0 - t * t * 4.0).sqrt() - 1.0) / -2.0
    } else {
        let t = t * 2.0 - 2.0;
        ((1.0 - t * t).sqrt() + 1.0) / 2.0
    }
}

fn expo_in(t: f64) -> f64 {
    2.0_f64.powf(10.0 * (t - 1.0))
}

fn expo_out(t: f64) -> f64 {
    -2.0_f64.powf(-10.0 * t) + 1.0
}

fn expo_in_out(t: f64) -> f64 {
    if t < 0.5 {
        2.0_f64.powf(10.0 * (t * 2.0 - 1.0)) / 2.0
    } else {
        (-2.0_f64.powf(-10.0 * (t * 2.0 - 1.0)) + 2.0) / 2.0
    }
}

fn back_in(t: f64) -> f64 {
    t * t * (2.701_58 * t - 1.701_58)
}

fn back_out(t: f64) -> f64 {
    let t = t - 1.0;
    1.0 - t * t * (-2.701_58 * t - 1.701_58)
}

fn back_in_out(t: f64) -> f64 {
    let mut t = t * 2.0;
    if t < 1.0 {
        t * t * (2.701_58 * t - 1.701_58) / 2.0
    } else {
        t -= 1.0;
        let t = t - 1.0;
        (1.0 - t * t * (-2.701_58 * t - 1.701_58)) / 2.0 + 0.5
    }
}

fn elastic_in(t: f64) -> f64 {
    let t = t - 1.0;
    -(ELASTIC_AMPLITUDE
        * 2.0_f64.powf(10.0 * t)
        * f64::sin(
            (t - (ELASTIC_PERIOD / (2.0 * std::f64::consts::PI)
                * (1.0 / ELASTIC_AMPLITUDE).asin()))
                * (2.0 * std::f64::consts::PI)
                / ELASTIC_PERIOD,
        ))
}

fn elastic_out(t: f64) -> f64 {
    ELASTIC_AMPLITUDE
        * 2.0_f64.powf(-10.0 * t)
        * f64::sin(
            (t - (ELASTIC_PERIOD / (2.0 * std::f64::consts::PI)
                * (1.0 / ELASTIC_AMPLITUDE).asin()))
                * (2.0 * std::f64::consts::PI)
                / ELASTIC_PERIOD,
        )
        + 1.0
}

fn elastic_in_out(t: f64) -> f64 {
    if t < 0.5 {
        let t = t - 0.5;
        -0.5 * (2.0_f64.powf(10.0 * t)
            * f64::sin((t - ELASTIC_PERIOD / 4.0) * (2.0 * std::f64::consts::PI) / ELASTIC_PERIOD))
    } else {
        let t = t - 0.5;
        2.0_f64.powf(-10.0 * t)
            * f64::sin((t - ELASTIC_PERIOD / 4.0) * (2.0 * std::f64::consts::PI) / ELASTIC_PERIOD)
            * 0.5
            + 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EASE_NAMES: [&str; 37] = [
        "linear",
        "quadIn",
        "quadOut",
        "quadInOut",
        "cubeIn",
        "cubeOut",
        "cubeInOut",
        "quartIn",
        "quartOut",
        "quartInOut",
        "quintIn",
        "quintOut",
        "quintInOut",
        "sineIn",
        "sineOut",
        "sineInOut",
        "expoIn",
        "expoOut",
        "expoInOut",
        "circIn",
        "circOut",
        "circInOut",
        "backIn",
        "backOut",
        "backInOut",
        "bounceIn",
        "bounceOut",
        "bounceInOut",
        "smoothStepIn",
        "smoothStepOut",
        "smoothStepInOut",
        "smootherStepIn",
        "smootherStepOut",
        "smootherStepInOut",
        "elasticIn",
        "elasticOut",
        "elasticInOut",
    ];

    fn close(context: &str, actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "{context}: expected {expected}, got {actual}"
        );
    }

    /// Endpoint values matching the transcribed flixel 6.1.2 formulas
    /// (FlxEase.hx L190-203, L224-244): the expo family starts/ends at
    /// 2^-10 offsets and two elastic variants overshoot — elasticIn(0) =
    /// -2^-10 (the `t -= 1` phase lands on a 5.5π sine) and
    /// elasticOut(1) = 1 + 2^-10 (a 4.5π sine) — unlike the exact 0/1 of
    /// the polynomial and elasticInOut families. Pinning the observed
    /// native values instead of an idealized endpoint invariant.
    fn endpoint(name: &str) -> (f64, f64) {
        const EPS: f64 = 9.765_625e-4; // 2^-10
        match name {
            "expoIn" => (EPS, 1.0),
            "expoOut" => (0.0, 1.0 - EPS),
            "expoInOut" => (EPS / 2.0, 1.0 - EPS / 2.0),
            "elasticIn" => (-EPS, 1.0),
            "elasticOut" => (0.0, 1.0 + EPS),
            _ => (0.0, 1.0),
        }
    }

    #[test]
    fn full_flx_ease_family_resolves_and_hits_endpoints() {
        for name in EASE_NAMES {
            let (resolved, function) =
                resolve_ease(name).unwrap_or_else(|| panic!("{name} must resolve"));
            assert_eq!(resolved, name);
            let (low, high) = endpoint(name);
            close(name, function(0.0), low);
            close(name, function(1.0), high);
        }
        assert!(resolve_ease("quadin").is_none(), "lookup is case-sensitive");
        assert!(resolve_ease("notAnEase").is_none());
    }

    #[test]
    fn representative_ease_formulas_match_the_transcription() {
        close("quadIn", quad_in(0.5), 0.25);
        close("quadOut", quad_out(0.5), 0.75);
        close("quadInOut", quad_in_out(0.25), 0.125);
        close("cubeIn", cube_in(0.5), 0.125);
        close("quartOut", quart_out(0.5), 1.0 - 0.5_f64.powi(4));
        close("quintIn", quint_in(0.5), 0.5_f64.powi(5));
        close("sineIn", sine_in(1.0), 1.0);
        close("sineOut", sine_out(0.0), 0.0);
        close("sineInOut", sine_in_out(0.5), 0.5);
        close("expoIn", expo_in(1.0), 1.0);
        close("expoOut", expo_out(0.0), 0.0);
        close("circIn", circ_in(1.0), 1.0);
        close("circOut", circ_out(0.0), 0.0);
        close("backIn", back_in(1.0), 1.0);
        close("backOut", back_out(0.0), 0.0);
        close("bounceOut", bounce_out(1.0), 1.0);
        close("smoothStepInOut", smooth_step_in_out(0.5), 0.5);
        close("smootherStepInOut", smoother_step_in_out(0.5), 0.5);
        close("elasticOut", elastic_out(0.0), 0.0);
        close("elasticIn", elastic_in(1.0), 1.0);
        close("elasticInOut", elastic_in_out(1.0), 1.0);
    }
}
