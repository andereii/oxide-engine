//! Structured script failures shown to modders (file, line, message).

use super::backend::{ScriptError, ScriptLoc};

pub const SCRIPT_ERROR_HINT: &str = "Backspace: dismiss    Shift+S: log";

const LOG_ENTRY_LIMIT: usize = 32;
const MESSAGE_CHAR_LIMIT: usize = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptDiagKind {
    Compile,
    Runtime,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptDiagnostic {
    pub kind: ScriptDiagKind,
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub hook: Option<String>,
    pub message: String,
    pub repeats: u32,
    pub seq: u64,
}

impl ScriptDiagnostic {
    pub fn from_error(err: &ScriptError) -> Self {
        match err {
            ScriptError::Io { path, source } => Self {
                kind: ScriptDiagKind::Io,
                file: path.display().to_string(),
                line: None,
                column: None,
                hook: None,
                message: source.to_string(),
                repeats: 1,
                seq: 0,
            },
            ScriptError::UnsupportedExtension(ext) => Self {
                kind: ScriptDiagKind::Compile,
                file: format!(".{ext}"),
                line: None,
                column: None,
                hook: None,
                message: format!("no scripting backend registered for extension '.{ext}'"),
                repeats: 1,
                seq: 0,
            },
            ScriptError::Compile {
                script_id,
                loc,
                message,
            } => Self::located(ScriptDiagKind::Compile, script_id, loc, None, message),
            ScriptError::Runtime {
                script_id,
                loc,
                hook,
                message,
            } => Self::located(
                ScriptDiagKind::Runtime,
                script_id,
                loc,
                Some(hook.clone()),
                message,
            ),
        }
    }

    fn located(
        kind: ScriptDiagKind,
        script_id: &str,
        loc: &ScriptLoc,
        hook: Option<String>,
        message: &str,
    ) -> Self {
        let file = if loc.file.is_empty() {
            script_id.to_string()
        } else {
            loc.file.clone()
        };
        Self {
            kind,
            file,
            line: loc.line,
            column: loc.column,
            hook,
            message: message.to_string(),
            repeats: 1,
            seq: 0,
        }
    }

    pub fn same_error(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.file == other.file
            && self.line == other.line
            && self.column == other.column
            && self.hook == other.hook
            && self.message == other.message
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptErrorAction {
    None,
    Dismiss,
    ToggleLog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptErrorView {
    pub epoch: u64,
    pub acked_seq: u64,
    pub banner: bool,
    pub log_open: bool,
}

impl Default for ScriptErrorView {
    fn default() -> Self {
        Self {
            epoch: 0,
            acked_seq: 0,
            banner: false,
            log_open: false,
        }
    }
}

impl ScriptErrorView {
    pub fn panel_open(&self) -> bool {
        self.banner || self.log_open
    }
}

/// Advances the on-screen error panel. `epoch` changes when the script log is cleared.
/// `latest_seq` is 0 when there are no diagnostics.
pub fn step_script_error_view(
    view: &mut ScriptErrorView,
    epoch: u64,
    latest_seq: u64,
    action: ScriptErrorAction,
) {
    if view.epoch != epoch {
        view.epoch = epoch;
        view.acked_seq = 0;
        view.banner = false;
        view.log_open = false;
    }
    if latest_seq > view.acked_seq {
        view.banner = true;
    }
    match action {
        ScriptErrorAction::Dismiss if view.panel_open() => {
            view.banner = false;
            view.log_open = false;
            view.acked_seq = latest_seq;
        }
        ScriptErrorAction::ToggleLog => {
            view.log_open = !view.log_open;
            if view.log_open {
                view.banner = true;
            } else {
                view.banner = latest_seq > view.acked_seq;
            }
        }
        ScriptErrorAction::None | ScriptErrorAction::Dismiss => {}
    }
    if latest_seq == 0 && !view.log_open {
        view.banner = false;
    }
}

pub fn format_script_error_banner(diags: &[ScriptDiagnostic]) -> String {
    let Some(latest) = diags.last() else {
        return String::new();
    };
    let mut out = String::new();
    if diags.len() > 1 {
        let _ = writeln_count(&mut out, diags.len());
    }
    push_diag(&mut out, latest);
    out
}

pub fn format_script_error_log(diags: &[ScriptDiagnostic]) -> String {
    let mut out = String::from("script log");
    if diags.is_empty() {
        out.push_str("\nNo script errors.");
        return out;
    }
    let skip = diags.len().saturating_sub(LOG_ENTRY_LIMIT);
    if skip > 0 {
        out.push_str(&format!("  (last {LOG_ENTRY_LIMIT} of {})", diags.len()));
    }
    out.push('\n');
    for diag in diags[skip..].iter().rev() {
        out.push('\n');
        push_diag(&mut out, diag);
        out.push('\n');
    }
    out
}

fn writeln_count(out: &mut String, count: usize) {
    out.push_str(&format!("{count} script errors. Latest:\n"));
}

fn push_diag(out: &mut String, diag: &ScriptDiagnostic) {
    out.push_str(&location_line(diag));
    out.push('\n');
    if let Some(hook) = &diag.hook {
        out.push_str("hook ");
        out.push_str(hook);
        out.push('\n');
    }
    out.push_str(&truncate_msg(&diag.message));
    if diag.repeats > 1 {
        out.push_str("\n(repeating)");
    }
}

fn location_line(diag: &ScriptDiagnostic) -> String {
    let kind = match diag.kind {
        ScriptDiagKind::Compile => "compile",
        ScriptDiagKind::Runtime => "runtime",
        ScriptDiagKind::Io => "io",
    };
    match (diag.line, diag.column) {
        (Some(line), Some(column)) => format!("{kind}  {}:{line}:{column}", diag.file),
        (Some(line), None) => format!("{kind}  {}:{line}", diag.file),
        _ => format!("{kind}  {}", diag.file),
    }
}

fn truncate_msg(msg: &str) -> String {
    let mut count = 0;
    for (idx, _) in msg.char_indices() {
        if count == MESSAGE_CHAR_LIMIT {
            return format!("{}…", &msg[..idx]);
        }
        count += 1;
    }
    msg.to_string()
}

/// Splits a trailing Rhai `(line N, position M)` suffix off an error string.
/// Position 0 means the beginning of the line and is reported as no column.
pub(crate) fn split_rhai_position_suffix(raw: &str) -> (String, Option<u32>, Option<u32>) {
    let Some(start) = raw.rfind(" (line ") else {
        return (raw.to_string(), None, None);
    };
    let rest = &raw[start + " (line ".len()..];
    let Some((line, after_line)) = take_u32(rest) else {
        return (raw.to_string(), None, None);
    };
    let Some(after_label) = after_line.strip_prefix(", position ") else {
        return (raw.to_string(), None, None);
    };
    let Some((pos, after_pos)) = take_u32(after_label) else {
        return (raw.to_string(), None, None);
    };
    let Some(after) = after_pos.strip_prefix(')') else {
        return (raw.to_string(), None, None);
    };
    if !(after.is_empty() || after.starts_with('\n')) {
        return (raw.to_string(), None, None);
    }
    let mut message = String::with_capacity(start + after.len());
    message.push_str(&raw[..start]);
    message.push_str(after);
    let column = if pos == 0 { None } else { Some(pos) };
    let line = if line == 0 { None } else { Some(line) };
    (message.trim_end().to_string(), line, column)
}

fn take_u32(text: &str) -> Option<(u32, &str)> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let (num, rest) = text.split_at(digits);
    let value = num.parse().ok()?;
    Some((value, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(seq: u64, line: u32) -> ScriptDiagnostic {
        ScriptDiagnostic {
            kind: ScriptDiagKind::Runtime,
            file: "demo/bad.rhai".to_string(),
            line: Some(line),
            column: Some(4),
            hook: Some("onUpdate".to_string()),
            message: "Variable not found: foo".to_string(),
            repeats: 1,
            seq,
        }
    }

    #[test]
    fn banner_names_file_line_and_hint_keys_live_with_the_panel() {
        let text = format_script_error_banner(&[sample(1, 18)]);
        assert!(text.contains("demo/bad.rhai:18:4"), "{text}");
        assert!(text.contains("runtime"), "{text}");
        assert!(text.contains("hook onUpdate"), "{text}");
        assert!(text.contains("Variable not found: foo"), "{text}");
        assert!(SCRIPT_ERROR_HINT.contains("Backspace"));
        assert!(SCRIPT_ERROR_HINT.contains("Shift+S"));
    }

    #[test]
    fn log_lists_newest_first_and_empty_log_is_explicit() {
        let text = format_script_error_log(&[sample(1, 2), sample(2, 9)]);
        let first = text.find("bad.rhai:9:4").expect("newest");
        let second = text.find("bad.rhai:2:4").expect("older");
        assert!(first < second, "{text}");
        let empty = format_script_error_log(&[]);
        assert!(empty.contains("No script errors."), "{empty}");
    }

    #[test]
    fn dismiss_hides_until_a_new_error_and_shift_s_toggles_the_log() {
        let mut view = ScriptErrorView::default();
        step_script_error_view(&mut view, 1, 1, ScriptErrorAction::None);
        assert!(view.banner);
        assert!(!view.log_open);

        step_script_error_view(&mut view, 1, 1, ScriptErrorAction::Dismiss);
        assert!(!view.panel_open());

        step_script_error_view(&mut view, 1, 1, ScriptErrorAction::None);
        assert!(!view.panel_open());

        step_script_error_view(&mut view, 1, 2, ScriptErrorAction::None);
        assert!(view.banner);

        step_script_error_view(&mut view, 1, 2, ScriptErrorAction::ToggleLog);
        assert!(view.log_open);
        step_script_error_view(&mut view, 1, 2, ScriptErrorAction::ToggleLog);
        assert!(!view.log_open);
        assert!(view.banner);

        step_script_error_view(&mut view, 1, 2, ScriptErrorAction::Dismiss);
        step_script_error_view(&mut view, 1, 0, ScriptErrorAction::ToggleLog);
        assert!(view.log_open);
        step_script_error_view(&mut view, 1, 0, ScriptErrorAction::ToggleLog);
        assert!(!view.panel_open());
    }

    #[test]
    fn reload_epoch_shows_the_new_failure() {
        let mut view = ScriptErrorView::default();
        step_script_error_view(&mut view, 1, 3, ScriptErrorAction::Dismiss);
        assert!(!view.panel_open());
        step_script_error_view(&mut view, 2, 1, ScriptErrorAction::None);
        assert!(view.banner);
        assert_eq!(view.acked_seq, 0);
    }

    #[test]
    fn repeating_banner_does_not_change_as_the_count_grows() {
        let mut first = sample(1, 4);
        first.repeats = 2;
        let mut later = first.clone();
        later.repeats = 4510;
        assert_eq!(
            format_script_error_banner(std::slice::from_ref(&first)),
            format_script_error_banner(std::slice::from_ref(&later))
        );
        assert!(format_script_error_banner(std::slice::from_ref(&later)).contains("(repeating)"));
    }

    #[test]
    fn rhai_suffix_keeps_the_inner_line_and_reports_the_outer_one() {
        let raw = "Variable not found: foo (line 4, position 2)\nin call to function 'onUpdate' (line 9, position 0)";
        let (message, line, column) = split_rhai_position_suffix(raw);
        assert_eq!(line, Some(9));
        assert_eq!(column, None);
        assert!(message.contains("(line 4, position 2)"), "{message}");
        assert!(message.contains("onUpdate"), "{message}");
        assert!(!message.contains("(line 9"), "{message}");
    }
}
