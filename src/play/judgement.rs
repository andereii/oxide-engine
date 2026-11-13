use crate::utils::constants::{
    HEALTH_BAD_BONUS, HEALTH_GOOD_BONUS, HEALTH_SHIT_BONUS, HEALTH_SICK_BONUS,
    JUDGEMENT_BAD_COMBO_BREAK, JUDGEMENT_GOOD_COMBO_BREAK, JUDGEMENT_SHIT_COMBO_BREAK,
    JUDGEMENT_SICK_COMBO_BREAK, PBOT1_BAD_THRESHOLD, PBOT1_GOOD_THRESHOLD, PBOT1_MAX_SCORE,
    PBOT1_MIN_SCORE, PBOT1_MISS_SCORE, PBOT1_PERFECT_THRESHOLD, PBOT1_SCORING_OFFSET,
    PBOT1_SCORING_SLOPE, PBOT1_SHIT_THRESHOLD, PBOT1_SICK_THRESHOLD,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judgement {
    Sick,
    Good,
    Bad,
    Shit,
    Miss,
}

impl Judgement {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sick => "sick",
            Self::Good => "good",
            Self::Bad => "bad",
            Self::Shit => "shit",
            Self::Miss => "miss",
        }
    }

    pub fn health_change(self) -> f64 {
        match self {
            Self::Sick => HEALTH_SICK_BONUS,
            Self::Good => HEALTH_GOOD_BONUS,
            Self::Bad => HEALTH_BAD_BONUS,
            Self::Shit => HEALTH_SHIT_BONUS,
            Self::Miss => 0.0,
        }
    }

    pub fn is_combo_break(self) -> bool {
        match self {
            Self::Sick => JUDGEMENT_SICK_COMBO_BREAK,
            Self::Good => JUDGEMENT_GOOD_COMBO_BREAK,
            Self::Bad => JUDGEMENT_BAD_COMBO_BREAK,
            Self::Shit => JUDGEMENT_SHIT_COMBO_BREAK,
            Self::Miss => true,
        }
    }

    pub fn record(self, tallies: &mut Tallies) {
        match self {
            Self::Sick => tallies.sick += 1,
            Self::Good => tallies.good += 1,
            Self::Bad => tallies.bad += 1,
            Self::Shit => tallies.shit += 1,
            Self::Miss => tallies.missed += 1,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tallies {
    pub sick: u32,
    pub good: u32,
    pub bad: u32,
    pub shit: u32,
    pub missed: u32,
    pub combo: u32,
    pub max_combo: u32,
    pub total_notes_hit: u32,
    pub total_notes: u32,
}

pub fn judge_note(note_diff_ms: f64) -> Judgement {
    let abs_timing = note_diff_ms.abs();
    if abs_timing <= PBOT1_SICK_THRESHOLD {
        Judgement::Sick
    } else if abs_timing <= PBOT1_GOOD_THRESHOLD {
        Judgement::Good
    } else if abs_timing <= PBOT1_BAD_THRESHOLD {
        Judgement::Bad
    } else if abs_timing <= PBOT1_SHIT_THRESHOLD {
        Judgement::Shit
    } else {
        Judgement::Miss
    }
}

pub fn score_note(note_diff_ms: f64) -> i32 {
    let abs_timing = note_diff_ms.abs();
    if abs_timing > PBOT1_SHIT_THRESHOLD {
        return PBOT1_MISS_SCORE;
    }
    if abs_timing <= PBOT1_PERFECT_THRESHOLD {
        return PBOT1_MAX_SCORE;
    }
    let factor = 1.0 - (1.0 / (1.0 + (-PBOT1_SCORING_SLOPE * (abs_timing - PBOT1_SCORING_OFFSET)).exp()));
    (f64::from(PBOT1_MAX_SCORE) * factor + PBOT1_MIN_SCORE).round() as i32
}

pub fn miss_score() -> i32 {
    PBOT1_MISS_SCORE
}

