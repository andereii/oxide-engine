use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::report::{BUG_REPORT_URL, ENGINE_NAME, ENGINE_VERSION};

/// Embedded fallback copy of the crash banner GIF so the dialog still animates even if the
/// assets folder cannot be resolved during an early crash.
const EMBEDDED_CRASH_GIF: &[u8] = include_bytes!("../../assets/ui/crash/cat_exploding.gif");

/// Resolves `assets/ui/crash/cat_exploding.gif`, falling back to a temp file written from the
/// embedded bytes when assets are unreachable.
pub(crate) fn resolve_or_extract_gif() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("assets/ui/crash/cat_exploding.gif"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("assets/ui/crash/cat_exploding.gif")))
            .unwrap_or_default(),
    ];
    for c in candidates {
        if !c.as_os_str().is_empty() && c.is_file() {
            return Some(c);
        }
    }
    let tmp = std::env::temp_dir().join("oxide_crash_cat_exploding.gif");
    if !tmp.is_file() {
        let _ = fs::write(&tmp, EMBEDDED_CRASH_GIF);
    }
    tmp.is_file().then_some(tmp)
}

/// Launches the out-of-process native OS crash dialog (Qt on Linux, Windows Forms on Windows).
/// Returns `true` if a native dialog process was started and exited cleanly.
pub fn show_native_crash_dialog(
    report_path: &Path,
    gif_path: Option<&Path>,
    exe_path: &Path,
    restart_args: &[String],
) -> bool {
    let title = format!("{ENGINE_NAME} {ENGINE_VERSION} - Fatal error");
    let gif_str = gif_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let report_str = report_path.to_string_lossy().into_owned();
    let exe_str = exe_path.to_string_lossy().into_owned();
    let args_json = serde_json::to_string(restart_args).unwrap_or_else(|_| "[]".to_string());

    #[cfg(target_os = "linux")]
    {
        if launch_linux_qt_dialog(&title, &report_str, &gif_str, &exe_str, &args_json) {
            return true;
        }
        launch_linux_fallback_dialog(&title, &report_str)
    }

    #[cfg(target_os = "windows")]
    {
        launch_windows_forms_dialog(&title, &report_str, &gif_str, &exe_str, &args_json)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (title, gif_str, report_str, exe_str, args_json);
        false
    }
}

#[cfg(target_os = "linux")]
const LINUX_QT_DIALOG_PY: &str = r##"
import json
import os
import subprocess
import sys
from datetime import datetime
from pathlib import Path

QtCore = QtGui = QtWidgets = None
for mod in ("PySide6", "PyQt6", "PyQt5"):
    try:
        pkg = __import__(mod, fromlist=["QtCore", "QtGui", "QtWidgets"])
        QtCore, QtGui, QtWidgets = pkg.QtCore, pkg.QtGui, pkg.QtWidgets
        break
    except Exception:
        continue

if QtWidgets is None:
    sys.exit(12)

title = sys.argv[1]
report_path = sys.argv[2]
gif_path = sys.argv[3]
exe_path = sys.argv[4]
restart_args = json.loads(sys.argv[5]) if len(sys.argv) > 5 else []
bug_url = sys.argv[6] if len(sys.argv) > 6 else "https://github.com/andereii/oxide-engine/issues/new"
screenshot_out = os.environ.get("OXIDE_CRASH_DIALOG_SCREENSHOT", "").strip()

report_text = Path(report_path).read_text(encoding="utf-8", errors="replace") if Path(report_path).is_file() else "(crash report missing)"

app = QtWidgets.QApplication([title])

def make_copy_icon(palette):
    fg = palette.color(QtGui.QPalette.ColorRole.ButtonText)
    bg = palette.color(QtGui.QPalette.ColorRole.Button)
    pm = QtGui.QPixmap(20, 20)
    pm.fill(QtCore.Qt.GlobalColor.transparent)
    p = QtGui.QPainter(pm)
    p.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
    pen = QtGui.QPen(fg, 1.6)
    p.setPen(pen)
    p.drawRoundedRect(7, 2, 10, 12, 1.5, 1.5)
    p.fillRect(3, 6, 10, 12, bg)
    p.drawRoundedRect(3, 6, 10, 12, 1.5, 1.5)
    p.end()
    return QtGui.QIcon(pm)

def make_save_icon(palette):
    fg = palette.color(QtGui.QPalette.ColorRole.ButtonText)
    pm = QtGui.QPixmap(20, 20)
    pm.fill(QtCore.Qt.GlobalColor.transparent)
    p = QtGui.QPainter(pm)
    p.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
    pen = QtGui.QPen(fg, 1.6)
    p.setPen(pen)
    poly = QtGui.QPolygonF([
        QtCore.QPointF(3, 2), QtCore.QPointF(14, 2),
        QtCore.QPointF(17, 5), QtCore.QPointF(17, 17),
        QtCore.QPointF(3, 17),
    ])
    p.drawPolygon(poly)
    p.drawRect(6, 2, 7, 5)
    p.drawRect(6, 11, 8, 6)
    p.end()
    return QtGui.QIcon(pm)

dlg = QtWidgets.QDialog()
dlg.setWindowTitle(title)
dlg.resize(780, 560)
dlg.setMinimumSize(620, 440)

root = QtWidgets.QVBoxLayout(dlg)
root.setContentsMargins(0, 0, 0, 0)
root.setSpacing(0)

# Top dark banner (animated cat GIF on left + Uh oh... text on right)
banner = QtWidgets.QFrame()
banner.setObjectName("CrashBanner")
banner.setFixedHeight(114)
banner.setStyleSheet(
    "QFrame#CrashBanner {"
    "  background: qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 #1d1d1d, stop:1 #3d3d3d);"
    "  border-bottom: 1px solid #181818;"
    "}"
)
banner_layout = QtWidgets.QHBoxLayout(banner)
banner_layout.setContentsMargins(0, 0, 18, 0)
banner_layout.setSpacing(18)

gif_label = QtWidgets.QLabel()
gif_label.setFixedSize(114, 114)
gif_label.setStyleSheet("background: #141414; border: none;")
gif_label.setScaledContents(True)
movie = None
if gif_path and Path(gif_path).is_file():
    movie = QtGui.QMovie(gif_path)
    movie.setScaledSize(QtCore.QSize(114, 114))
    gif_label.setMovie(movie)
    movie.start()
banner_layout.addWidget(gif_label)

text_box = QtWidgets.QVBoxLayout()
text_box.setContentsMargins(0, 14, 0, 14)
text_box.setSpacing(6)

heading = QtWidgets.QLabel("Uh oh...")
heading.setStyleSheet("color: #ffffff; font-size: 20px; font-weight: bold; background: transparent; border: none;")
text_box.addWidget(heading)

subtitle = QtWidgets.QLabel(
    "Something crashed inside Oxide Engine. Please contact us so we can fix the problem as quickly\n"
    "as possible. If you were modifying the engine, well, good luck :)"
)
subtitle.setWordWrap(True)
subtitle.setStyleSheet("color: #e6e6e6; font-size: 13px; background: transparent; border: none;")
text_box.addWidget(subtitle)
text_box.addStretch(1)

banner_layout.addLayout(text_box, 1)
root.addWidget(banner)

# Main body (native OS palette)
body = QtWidgets.QVBoxLayout()
body.setContentsMargins(16, 16, 16, 14)
body.setSpacing(10)

report_edit = QtWidgets.QPlainTextEdit()
report_edit.setReadOnly(True)
report_edit.setPlainText(report_text)
mono_font = QtGui.QFontDatabase.systemFont(QtGui.QFontDatabase.SystemFont.FixedFont)
mono_font.setPointSize(10)
report_edit.setFont(mono_font)
report_edit.document().setDocumentMargin(6)
body.addWidget(report_edit, 1)

# Utility icon row directly beneath the report box (Copy + Save)
util_row = QtWidgets.QHBoxLayout()
util_row.setContentsMargins(0, 0, 0, 4)
util_row.setSpacing(6)

status_label = QtWidgets.QLabel("")
util_row.addWidget(status_label)
util_row.addStretch(1)

copy_btn = QtWidgets.QToolButton()
copy_btn.setIcon(make_copy_icon(dlg.palette()))
copy_btn.setIconSize(QtCore.QSize(18, 18))
copy_btn.setFixedSize(32, 30)
copy_btn.setToolTip("Copy crash report to clipboard")

def on_copy():
    app.clipboard().setText(report_text)
    status_label.setText("Crash report copied to clipboard.")
    QtCore.QTimer.singleShot(2500, lambda: status_label.setText(""))

copy_btn.clicked.connect(on_copy)
util_row.addWidget(copy_btn)

save_btn = QtWidgets.QToolButton()
save_btn.setIcon(make_save_icon(dlg.palette()))
save_btn.setIconSize(QtCore.QSize(18, 18))
save_btn.setFixedSize(32, 30)
save_btn.setToolTip("Save crash report to file...")

def on_save():
    default_name = f"oxide-crash-{datetime.utcnow().strftime('%Y%m%d-%H%M%S')}.txt"
    dest, _ = QtWidgets.QFileDialog.getSaveFileName(
        dlg, "Save Crash Report", default_name, "Text Files (*.txt);;Log Files (*.log);;All Files (*)"
    )
    if dest:
        try:
            Path(dest).write_text(report_text, encoding="utf-8")
            status_label.setText(f"Saved to {Path(dest).name}")
        except Exception as err:
            status_label.setText(f"Save failed: {err}")

save_btn.clicked.connect(on_save)
util_row.addWidget(save_btn)
body.addLayout(util_row)

# Bottom action buttons (Report bug | Restart Oxide | Close)
btn_row = QtWidgets.QHBoxLayout()
btn_row.setContentsMargins(0, 4, 0, 0)
btn_row.setSpacing(10)
btn_row.addStretch(1)

report_btn = QtWidgets.QPushButton("Report bug")
report_btn.setMinimumWidth(104)
report_btn.setMinimumHeight(30)
def on_report():
    QtGui.QDesktopServices.openUrl(QtCore.QUrl(bug_url))
report_btn.clicked.connect(on_report)
btn_row.addWidget(report_btn)

restart_btn = QtWidgets.QPushButton("Restart Oxide")
restart_btn.setMinimumWidth(116)
restart_btn.setMinimumHeight(30)
def on_restart():
    if exe_path and Path(exe_path).is_file():
        subprocess.Popen([exe_path, *restart_args], start_new_session=True)
    dlg.accept()
restart_btn.clicked.connect(on_restart)
btn_row.addWidget(restart_btn)

close_btn = QtWidgets.QPushButton("Close")
close_btn.setMinimumWidth(92)
close_btn.setMinimumHeight(30)
close_btn.setDefault(True)
close_btn.clicked.connect(dlg.reject)
btn_row.addWidget(close_btn)

body.addLayout(btn_row)
root.addLayout(body, 1)

if screenshot_out:
    def snap_and_close():
        dlg.grab().save(screenshot_out)
        dlg.reject()
    QtCore.QTimer.singleShot(350, snap_and_close)

dlg.exec()
"##;

#[cfg(target_os = "linux")]
fn launch_linux_qt_dialog(
    title: &str,
    report_path: &str,
    gif_path: &str,
    exe_path: &str,
    args_json: &str,
) -> bool {
    let status = Command::new("python3")
        .args([
            "-c",
            LINUX_QT_DIALOG_PY,
            title,
            report_path,
            gif_path,
            exe_path,
            args_json,
            BUG_REPORT_URL,
        ])
        .stdin(Stdio::null())
        .status();
    match status {
        Ok(s) if s.code() != Some(12) => true,
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn launch_linux_fallback_dialog(title: &str, report_path: &str) -> bool {
    if Command::new("kdialog")
        .args(["--title", title, "--textbox", report_path, "760", "520"])
        .status()
        .is_ok_and(|s| s.success())
    {
        return true;
    }
    Command::new("zenity")
        .args([
            "--text-info",
            "--title",
            title,
            "--filename",
            report_path,
            "--width=760",
            "--height=520",
        ])
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(target_os = "windows")]
const WINDOWS_FORMS_PS1: &str = r##"
param(
    [string]$Title,
    [string]$ReportPath,
    [string]$GifPath,
    [string]$ExePath,
    [string]$ArgsJson,
    [string]$BugUrl
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$reportText = if (Test-Path $ReportPath) { Get-Content -Raw -Encoding UTF8 $ReportPath } else { "(crash report missing)" }
$restartArgs = @()
try { $restartArgs = ConvertFrom-Json $ArgsJson } catch { $restartArgs = @() }

$form = New-Object System.Windows.Forms.Form
$form.Text = $Title
$form.Size = New-Object System.Drawing.Size(780, 560)
$form.MinimumSize = New-Object System.Drawing.Size(620, 440)
$form.StartPosition = "CenterScreen"

# Top dark banner
$banner = New-Object System.Windows.Forms.Panel
$banner.Dock = "Top"
$banner.Height = 114
$banner.BackColor = [System.Drawing.Color]::FromArgb(42, 42, 42)
$form.Controls.Add($banner)

$pic = New-Object System.Windows.Forms.PictureBox
$pic.Location = New-Object System.Drawing.Point(0, 0)
$pic.Size = New-Object System.Drawing.Size(114, 114)
$pic.SizeMode = [System.Windows.Forms.PictureBoxSizeMode]::StretchImage
$pic.BackColor = [System.Drawing.Color]::FromArgb(20, 20, 20)
if ($GifPath -and (Test-Path $GifPath)) {
    $pic.Image = [System.Drawing.Image]::FromFile($GifPath)
}
$banner.Controls.Add($pic)

$lblTitle = New-Object System.Windows.Forms.Label
$lblTitle.Text = "Uh oh..."
$lblTitle.Location = New-Object System.Drawing.Point(132, 16)
$lblTitle.AutoSize = $true
$lblTitle.ForeColor = [System.Drawing.Color]::White
$lblTitle.Font = New-Object System.Drawing.Font("Segoe UI", 16, [System.Drawing.FontStyle]::Bold)
$banner.Controls.Add($lblTitle)

$lblDesc = New-Object System.Windows.Forms.Label
$lblDesc.Text = "Something crashed inside Oxide Engine. Please contact us so we can fix the problem as quickly as possible. If you were modifying the engine, well, good luck :)"
$lblDesc.Location = New-Object System.Drawing.Point(132, 52)
$lblDesc.Size = New-Object System.Drawing.Size(610, 52)
$lblDesc.Anchor = "Top, Left, Right"
$lblDesc.ForeColor = [System.Drawing.Color]::FromArgb(230, 230, 230)
$lblDesc.Font = New-Object System.Drawing.Font("Segoe UI", 9.5)
$banner.Controls.Add($lblDesc)

# Scrollable read-only crash report box
$txt = New-Object System.Windows.Forms.TextBox
$txt.Multiline = $true
$txt.ReadOnly = $true
$txt.WordWrap = $false
$txt.ScrollBars = "Both"
$txt.BackColor = [System.Drawing.SystemColors]::Window
$txt.ForeColor = [System.Drawing.SystemColors]::WindowText
$txt.Font = New-Object System.Drawing.Font("Consolas", 9.5)
$txt.Text = $reportText
$txt.Location = New-Object System.Drawing.Point(16, 130)
$txt.Size = New-Object System.Drawing.Size(732, 300)
$txt.Anchor = "Top, Bottom, Left, Right"
$form.Controls.Add($txt)

$statusLbl = New-Object System.Windows.Forms.Label
$statusLbl.Text = ""
$statusLbl.Location = New-Object System.Drawing.Point(16, 442)
$statusLbl.AutoSize = $true
$statusLbl.Anchor = "Bottom, Left"
$statusLbl.ForeColor = [System.Drawing.Color]::DimGray
$form.Controls.Add($statusLbl)

$btnCopy = New-Object System.Windows.Forms.Button
$btnCopy.Text = [char]0x2398
$btnCopy.Size = New-Object System.Drawing.Size(32, 28)
$btnCopy.Location = New-Object System.Drawing.Point(678, 438)
$btnCopy.Anchor = "Bottom, Right"
$btnCopy.Add_Click({
    [System.Windows.Forms.Clipboard]::SetText($reportText)
    $statusLbl.Text = "Crash report copied to clipboard."
})
$form.Controls.Add($btnCopy)

$btnSave = New-Object System.Windows.Forms.Button
$btnSave.Text = "S"
$btnSave.Size = New-Object System.Drawing.Size(32, 28)
$btnSave.Location = New-Object System.Drawing.Point(716, 438)
$btnSave.Anchor = "Bottom, Right"
$btnSave.Add_Click({
    $sfd = New-Object System.Windows.Forms.SaveFileDialog
    $sfd.Filter = "Text Files (*.txt)|*.txt|Log Files (*.log)|*.log|All Files (*.*)|*.*"
    $sfd.FileName = "oxide-crash-" + (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss") + ".txt"
    if ($sfd.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
        [System.IO.File]::WriteAllText($sfd.FileName, $reportText, [System.Text.Encoding]::UTF8)
        $statusLbl.Text = "Saved crash report."
    }
})
$form.Controls.Add($btnSave)

$btnReport = New-Object System.Windows.Forms.Button
$btnReport.Text = "Report bug"
$btnReport.Size = New-Object System.Drawing.Size(102, 28)
$btnReport.Location = New-Object System.Drawing.Point(416, 480)
$btnReport.Anchor = "Bottom, Right"
$btnReport.Add_Click({ Start-Process $BugUrl })
$form.Controls.Add($btnReport)

$btnRestart = New-Object System.Windows.Forms.Button
$btnRestart.Text = "Restart Oxide"
$btnRestart.Size = New-Object System.Drawing.Size(114, 28)
$btnRestart.Location = New-Object System.Drawing.Point(526, 480)
$btnRestart.Anchor = "Bottom, Right"
$btnRestart.Add_Click({
    if ($ExePath -and (Test-Path $ExePath)) {
        if ($restartArgs.Count -gt 0) {
            Start-Process -FilePath $ExePath -ArgumentList $restartArgs
        } else {
            Start-Process -FilePath $ExePath
        }
    }
    $form.Close()
})
$form.Controls.Add($btnRestart)

$btnClose = New-Object System.Windows.Forms.Button
$btnClose.Text = "Close"
$btnClose.Size = New-Object System.Drawing.Size(100, 28)
$btnClose.Location = New-Object System.Drawing.Point(648, 480)
$btnClose.Anchor = "Bottom, Right"
$btnClose.Add_Click({ $form.Close() })
$form.AcceptButton = $btnClose
$form.CancelButton = $btnClose
$form.Controls.Add($btnClose)

[void]$form.ShowDialog()
"##;

#[cfg(target_os = "windows")]
fn launch_windows_forms_dialog(
    title: &str,
    report_path: &str,
    gif_path: &str,
    exe_path: &str,
    args_json: &str,
) -> bool {
    use std::os::windows::process::CommandExt;

    let script_path = std::env::temp_dir().join("oxide_crash_dialog.ps1");
    if fs::write(&script_path, WINDOWS_FORMS_PS1).is_err() {
        return false;
    }
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-STA",
        "-ExecutionPolicy",
        "Bypass",
        "-WindowStyle",
        "Hidden",
        "-File",
        &script_path.to_string_lossy(),
        "-Title",
        title,
        "-ReportPath",
        report_path,
        "-GifPath",
        gif_path,
        "-ExePath",
        exe_path,
        "-ArgsJson",
        args_json,
        "-BugUrl",
        BUG_REPORT_URL,
    ])
    .stdin(Stdio::null())
    .creation_flags(0x0800_0000);
    cmd.status().is_ok()
}
