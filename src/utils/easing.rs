use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EaseKind {
    #[default]
    Linear,
    SineIn,
    SineOut,
    SineInOut,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubeIn,
    CubeOut,
    CubeInOut,
    QuartIn,
    QuartOut,
    QuartInOut,
    QuintIn,
    QuintOut,
    QuintInOut,
    ExpoIn,
    ExpoOut,
    ExpoInOut,
    CircIn,
    CircOut,
    CircInOut,
    SmoothStepIn,
    SmoothStepOut,
    SmoothStepInOut,
    SmootherStepIn,
    SmootherStepOut,
    SmootherStepInOut,
    BackIn,
    BackOut,
    BackInOut,
    ElasticIn,
    ElasticOut,
    ElasticInOut,
    BounceIn,
    BounceOut,
    BounceInOut,
}

impl EaseKind {
    pub fn from_name(ease: &str, ease_dir: &str) -> Option<Self> {
        let e = ease.trim().to_ascii_lowercase();
        if e == "instant" {
            return None;
        }
        if e == "linear" || e.is_empty() {
            return Some(Self::Linear);
        }
        let d = ease_dir.trim().to_ascii_lowercase();
        let combined = if e.ends_with("inout") || e.ends_with("in") || e.ends_with("out") {
            e
        } else {
            format!("{e}{d}")
        };
        match combined.as_str() {
            "sinein" => Some(Self::SineIn),
            "sineout" => Some(Self::SineOut),
            "sineinout" => Some(Self::SineInOut),
            "quadin" => Some(Self::QuadIn),
            "quadout" => Some(Self::QuadOut),
            "quadinout" => Some(Self::QuadInOut),
            "cubein" => Some(Self::CubeIn),
            "cubeout" => Some(Self::CubeOut),
            "cubeinout" => Some(Self::CubeInOut),
            "quartin" => Some(Self::QuartIn),
            "quartout" => Some(Self::QuartOut),
            "quartinout" => Some(Self::QuartInOut),
            "quintin" => Some(Self::QuintIn),
            "quintout" => Some(Self::QuintOut),
            "quintinout" => Some(Self::QuintInOut),
            "expoin" => Some(Self::ExpoIn),
            "expoout" => Some(Self::ExpoOut),
            "expoinout" => Some(Self::ExpoInOut),
            "circin" => Some(Self::CircIn),
            "circout" => Some(Self::CircOut),
            "circinout" => Some(Self::CircInOut),
            "smoothstepin" => Some(Self::SmoothStepIn),
            "smoothstepout" => Some(Self::SmoothStepOut),
            "smoothstepinout" => Some(Self::SmoothStepInOut),
            "smootherstepin" => Some(Self::SmootherStepIn),
            "smootherstepout" => Some(Self::SmootherStepOut),
            "smootherstepinout" => Some(Self::SmootherStepInOut),
            "backin" => Some(Self::BackIn),
            "backout" => Some(Self::BackOut),
            "backinout" => Some(Self::BackInOut),
            "elasticin" => Some(Self::ElasticIn),
            "elasticout" => Some(Self::ElasticOut),
            "elasticinout" => Some(Self::ElasticInOut),
            "bouncein" => Some(Self::BounceIn),
            "bounceout" => Some(Self::BounceOut),
            "bounceinout" => Some(Self::BounceInOut),
            _ => Some(Self::Linear),
        }
    }

    pub fn eval(self, raw_t: f64) -> f64 {
        let t = raw_t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::SineIn => 1.0 - (t * PI * 0.5).cos(),
            Self::SineOut => (t * PI * 0.5).sin(),
            Self::SineInOut => -((PI * t).cos() - 1.0) * 0.5,
            Self::QuadIn => t * t,
            Self::QuadOut => 1.0 - (1.0 - t) * (1.0 - t),
            Self::QuadInOut => in_out(t, |x| x * x, |x| 1.0 - (1.0 - x) * (1.0 - x)),
            Self::CubeIn => t * t * t,
            Self::CubeOut => 1.0 - (1.0 - t).powi(3),
            Self::CubeInOut => in_out(t, |x| x * x * x, |x| 1.0 - (1.0 - x).powi(3)),
            Self::QuartIn => t.powi(4),
            Self::QuartOut => 1.0 - (1.0 - t).powi(4),
            Self::QuartInOut => in_out(t, |x| x.powi(4), |x| 1.0 - (1.0 - x).powi(4)),
            Self::QuintIn => t.powi(5),
            Self::QuintOut => 1.0 - (1.0 - t).powi(5),
            Self::QuintInOut => in_out(t, |x| x.powi(5), |x| 1.0 - (1.0 - x).powi(5)),
            Self::ExpoIn => if t == 0.0 { 0.0 } else { 2.0_f64.powf(10.0 * (t - 1.0)) },
            Self::ExpoOut => if t == 1.0 { 1.0 } else { 1.0 - 2.0_f64.powf(-10.0 * t) },
            Self::ExpoInOut => expo_in_out(t),
            Self::CircIn => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
            Self::CircOut => (1.0 - (t - 1.0).powi(2)).max(0.0).sqrt(),
            Self::CircInOut => in_out(
                t,
                |x| 1.0 - (1.0 - x * x).max(0.0).sqrt(),
                |x| (1.0 - (x - 1.0).powi(2)).max(0.0).sqrt(),
            ),
            Self::SmoothStepIn => 2.0 * smooth_step(t * 0.5),
            Self::SmoothStepOut => 2.0 * smooth_step(t * 0.5 + 0.5) - 1.0,
            Self::SmoothStepInOut => smooth_step(t),
            Self::SmootherStepIn => 2.0 * smoother_step(t * 0.5),
            Self::SmootherStepOut => 2.0 * smoother_step(t * 0.5 + 0.5) - 1.0,
            Self::SmootherStepInOut => smoother_step(t),
            Self::BackIn => back_in(t),
            Self::BackOut => 1.0 - back_in(1.0 - t),
            Self::BackInOut => in_out(t, back_in, |x| 1.0 - back_in(1.0 - x)),
            Self::ElasticIn => elastic_in(t),
            Self::ElasticOut => 1.0 - elastic_in(1.0 - t),
            Self::ElasticInOut => in_out(t, elastic_in, |x| 1.0 - elastic_in(1.0 - x)),
            Self::BounceIn => 1.0 - bounce_out(1.0 - t),
            Self::BounceOut => bounce_out(t),
            Self::BounceInOut => in_out(t, |x| 1.0 - bounce_out(1.0 - x), bounce_out),
        }
    }
}

fn in_out(t: f64, f_in: fn(f64) -> f64, f_out: fn(f64) -> f64) -> f64 {
    if t < 0.5 {
        0.5 * f_in(t * 2.0)
    } else {
        0.5 + 0.5 * f_out(t * 2.0 - 1.0)
    }
}

fn expo_in_out(t: f64) -> f64 {
    if t == 0.0 || t == 1.0 {
        return t;
    }
    if t < 0.5 {
        0.5 * 2.0_f64.powf(20.0 * t - 10.0)
    } else {
        1.0 - 0.5 * 2.0_f64.powf(-20.0 * t + 10.0)
    }
}

fn smooth_step(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn smoother_step(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn back_in(t: f64) -> f64 {
    const C1: f64 = 1.70158;
    t * t * ((C1 + 1.0) * t - C1)
}

fn elastic_in(t: f64) -> f64 {
    if t == 0.0 || t == 1.0 {
        return t;
    }
    const P: f64 = 0.4;
    let s = P / 4.0;
    let inv = t - 1.0;
    -(2.0_f64.powf(10.0 * inv)) * ((inv - s) * (2.0 * PI) / P).sin()
}

fn bounce_out(mut t: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        t -= 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        t -= 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        t -= 2.625 / D1;
        N1 * t * t + 0.984375
    }
}
