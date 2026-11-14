use crate::play::types::SongEventData;
use crate::utils::conductor::Conductor;
use crate::utils::easing::EaseKind;

const INTEGRATE_SLICES: usize = 32;
const NORMAL_VELOCITY_RANGE: f64 = 0.0001;

#[derive(Debug, Clone)]
pub struct ScrollVelocitySegment {
    pub time_ms: f64,
    pub from: f64,
    pub to: f64,
    pub duration_ms: f64,
    pub ease: Option<EaseKind>,
    pub set_start: bool,
}

#[derive(Debug, Clone)]
pub struct VelocityStretch {
    pub start: f64,
    pub return_start: f64,
    pub end: f64,
    pub return_ease: Option<EaseKind>,
    pub add: f64,
}

#[derive(Debug, Clone)]
pub struct ScrollVelocityTimeline {
    pub segments: Vec<ScrollVelocitySegment>,
    raw_starts: Vec<f64>,
    stretches: Vec<VelocityStretch>,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollLagState {
    pub stretch_start: f64,
    pub lag_pixels: f64,
    pub lag_previous: f64,
    pub lag_song_time: f64,
}

impl Default for ScrollLagState {
    fn default() -> Self {
        Self {
            stretch_start: -1.0,
            lag_pixels: 0.0,
            lag_previous: 0.0,
            lag_song_time: 0.0,
        }
    }
}

impl ScrollVelocityTimeline {
    pub fn from_events(events: &[SongEventData], strumline: &str, conductor: &Conductor) -> Option<Self> {
        let mut sv_events: Vec<&SongEventData> = events
            .iter()
            .filter(|e| e.kind == "ScrollVelocity")
            .collect();
        if sv_events.is_empty() {
            return None;
        }
        sv_events.sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));

        let mut timeline = Self {
            segments: vec![ScrollVelocitySegment {
                time_ms: 0.0, from: 1.0, to: 1.0, duration_ms: 0.0, ease: None, set_start: false,
            }],
            raw_starts: Vec::new(),
            stretches: Vec::new(),
        };

        for event in sv_events {
            let target = event.strumline_target.as_str();
            if target != "both" && target != strumline {
                continue;
            }
            let time_ms = event.time_ms.max(0.0);
            while timeline.segments.len() > 1 && timeline.segments.last().is_some_and(|s| s.time_ms > time_ms) {
                timeline.segments.pop();
            }

            let cur_vel = timeline.velocity_at(time_ms);
            let from = if event.use_start { event.start_velocity.unwrap_or(cur_vel) } else { cur_vel };
            let velocity = event.scroll_velocity.unwrap_or(1.0);
            let ease_fn = EaseKind::from_name(&event.ease, &event.ease_dir);
            let steps = event.duration_steps.unwrap_or(4.0);
            let duration_ms = if ease_fn.is_some() && steps > 0.0 {
                let step = conductor.get_time_in_steps(time_ms);
                steps * (conductor.get_step_time_in_ms(step + 1.0) - conductor.get_step_time_in_ms(step))
            } else {
                0.0
            };

            timeline.segments.push(ScrollVelocitySegment {
                time_ms, from, to: velocity, duration_ms,
                ease: if duration_ms > 0.0 { ease_fn } else { None },
                set_start: event.use_start && duration_ms > 0.0,
            });
        }

        if timeline.segments.len() <= 1 {
            return None;
        }

        timeline.raw_starts = vec![0.0];
        for i in 1..timeline.segments.len() {
            let prev = &timeline.segments[i - 1];
            let next_start = timeline.raw_starts[i - 1]
                + integrate(prev, prev.time_ms, timeline.segments[i].time_ms);
            timeline.raw_starts.push(next_start);
        }
        timeline.build_stretches();
        Some(timeline)
    }

    pub fn has_events(&self) -> bool {
        self.segments.len() > 1
    }

    pub fn velocity_at(&self, time_ms: f64) -> f64 {
        let idx = self.segment_index_at(time_ms);
        velocity_of(&self.segments[idx], time_ms)
    }

    pub fn stretch_start_at(&self, time_ms: f64) -> f64 {
        if !self.has_events() || time_ms <= 0.0 {
            return -1.0;
        }
        for stretch in &self.stretches {
            if time_ms < stretch.start {
                break;
            }
            if time_ms < stretch.end {
                return stretch.start;
            }
        }
        -1.0
    }

    pub fn visual_time(&self, time_ms: f64) -> f64 {
        if !self.has_events() || time_ms <= 0.0 {
            return time_ms;
        }
        for stretch in &self.stretches {
            if time_ms < stretch.start {
                break;
            }
            if time_ms >= stretch.end {
                continue;
            }
            let visual = stretch.start + self.raw_time(time_ms) - self.raw_time(stretch.start);
            if time_ms < stretch.return_start {
                return visual;
            }
            let len = stretch.end - stretch.return_start;
            let progress = if len > 0.0 {
                let norm = (time_ms - stretch.return_start) / len;
                stretch.return_ease.map(|e| e.eval(norm)).unwrap_or(norm)
            } else {
                1.0
            };
            return visual + stretch.add * progress;
        }
        time_ms
    }

    pub fn resolve_visual_song_position(
        &self,
        song_time_ms: f64,
        scroll_speed: f64,
        lag: &mut ScrollLagState,
    ) -> f64 {
        let stretch_start = self.stretch_start_at(song_time_ms);
        let cur_lag = song_time_ms - self.visual_time(song_time_ms);
        if (stretch_start - lag.stretch_start).abs() > f64::EPSILON || song_time_ms < lag.lag_song_time {
            lag.stretch_start = stretch_start;
            lag.lag_pixels = cur_lag * scroll_speed;
        } else {
            lag.lag_pixels = step_lag(lag.lag_pixels, lag.lag_previous, cur_lag, scroll_speed);
        }
        lag.lag_previous = cur_lag;
        lag.lag_song_time = song_time_ms;
        if stretch_start < 0.0 {
            song_time_ms - cur_lag
        } else {
            lag_to_visual(song_time_ms, lag.lag_pixels, cur_lag, scroll_speed)
        }
    }

    fn build_stretches(&mut self) {
        self.stretches.clear();
        let mut index = 1;
        while index < self.segments.len() {
            if !is_away(&self.segments[index]) {
                index += 1;
                continue;
            }
            let start = self.segments[index].time_ms;
            let mut back = index;
            while back < self.segments.len() && !is_normal(self.segments[back].to) {
                back += 1;
            }
            if back >= self.segments.len() {
                self.stretches.push(VelocityStretch {
                    start, return_start: f64::INFINITY, end: f64::INFINITY, return_ease: None, add: 0.0,
                });
                break;
            }
            let returning = &self.segments[back];
            let mut end = returning.time_ms + returning.duration_ms;
            if back + 1 < self.segments.len() {
                end = end.min(self.segments[back + 1].time_ms);
            }
            let return_start = returning.time_ms;
            let return_ease = returning.ease;
            let add = (end - start) - (self.raw_time(end) - self.raw_time(start));
            self.stretches.push(VelocityStretch { start, return_start, end, return_ease, add });
            index = back + 1;
        }
    }

    fn segment_index_at(&self, time_ms: f64) -> usize {
        let mut low = 0usize;
        let mut high = self.segments.len().saturating_sub(1);
        while low < high {
            let mid = (low + high + 1) >> 1;
            if self.segments[mid].time_ms <= time_ms {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        low
    }

    fn raw_time(&self, time_ms: f64) -> f64 {
        let idx = self.segment_index_at(time_ms);
        let seg = &self.segments[idx];
        self.raw_starts[idx] + integrate(seg, seg.time_ms, time_ms)
    }
}

pub fn step_lag(lag_pixels: f64, previous_lag: f64, lag: f64, speed: f64) -> f64 {
    if lag.abs() < previous_lag.abs() && previous_lag.abs() > f64::EPSILON {
        lag_pixels * lag / previous_lag
    } else {
        lag_pixels + (lag - previous_lag) * speed
    }
}

pub fn lag_to_visual(time_ms: f64, lag_pixels: f64, lag: f64, speed: f64) -> f64 {
    if speed.abs() < NORMAL_VELOCITY_RANGE {
        time_ms - lag
    } else {
        time_ms - lag_pixels / speed
    }
}

fn is_normal(velocity: f64) -> bool {
    (velocity - 1.0).abs() <= NORMAL_VELOCITY_RANGE
}

fn is_away(seg: &ScrollVelocitySegment) -> bool {
    !is_normal(seg.to) || (seg.set_start && !is_normal(seg.from))
}

fn velocity_of(seg: &ScrollVelocitySegment, time_ms: f64) -> f64 {
    let Some(ease) = seg.ease else {
        return seg.to;
    };
    if seg.duration_ms <= 0.0 || time_ms >= seg.time_ms + seg.duration_ms {
        return seg.to;
    }
    if time_ms <= seg.time_ms {
        return seg.from;
    }
    seg.from + (seg.to - seg.from) * ease.eval((time_ms - seg.time_ms) / seg.duration_ms)
}

fn integrate(seg: &ScrollVelocitySegment, from: f64, to: f64) -> f64 {
    if to <= from {
        return 0.0;
    }
    if seg.ease.is_none() || seg.duration_ms <= 0.0 {
        return seg.to * (to - from);
    }
    let ease_end = to.min(seg.time_ms + seg.duration_ms);
    let mut total = 0.0;
    if ease_end > from {
        let slice = (ease_end - from) / INTEGRATE_SLICES as f64;
        for i in 0..INTEGRATE_SLICES {
            total += velocity_of(seg, from + slice * (i as f64 + 0.5)) * slice;
        }
    }
    if to > ease_end {
        total += seg.to * (to - ease_end.max(from));
    }
    total
}
