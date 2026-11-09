use clap::{Parser, Subcommand};
use std::env;
use std::ffi::{c_int, c_ulong, CString};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::ptr::null;
use x11::xfixes::{XFixesQueryExtension, XFixesSelectSelectionInput};
use x11::xlib::{
    self, False, XCreateSimpleWindow, XDefaultRootWindow, XEvent, XInternAtom, XNextEvent,
    XOpenDisplay,
};

/// Stands in for newlines so that every history entry fits on a single line
const PLACEHOLDER: &str = "<NEWLINE>";

/// XFixesSetSelectionOwnerNotifyMask: report whenever the selection's owner changes
const SET_SELECTION_OWNER_NOTIFY_MASK: c_ulong = 1;
/// XFixes reports its selection change as its first event after the extension's base
const SELECTION_NOTIFY: c_int = 0;

/// Clipboard history, picked from with fleximenu
#[derive(Parser)]
#[command(name = "fleximenu_clip", about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// copy the primary selection to the clipboard, and add it to history
    Add,
    /// copy stdin to the clipboard, and add it to history
    Out,
    /// pick from history with fleximenu, and copy the choice to the clipboard
    Sel,
    /// run in the background, adding every change to the clipboard to history
    Watch,
}

fn main() -> ExitCode {
    match run(Cli::parse().action) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("fleximenu_clip: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(action: Action) -> io::Result<()> {
    let path = history_path()?;
    let notification = match action {
        Action::Add => {
            ensure_history(&path)?;
            // record the real clipboard (what Ctrl+C copies) before the
            // primary selection overwrites it, then record the selection
            let clipboard = xclip_output(&["-o", "-selection", "clipboard"]).unwrap_or_default();
            let primary = xclip_output(&["-o", "-selection", "primary"]).unwrap_or_default();
            let from_clipboard = record(&path, &clipboard)?;
            let from_primary = record(&path, &primary)?;
            if !primary.trim_end_matches('\n').is_empty() {
                set_clipboard(&primary)?;
            }
            from_primary.or(from_clipboard)
        }
        Action::Out => {
            ensure_history(&path)?;
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            set_clipboard(&text)?;
            record(&path, &text)?
        }
        Action::Sel => select(&path)?,
        Action::Watch => {
            watch(&path)?;
            None
        }
    };
    if let Some(message) = notification {
        notify(&message);
    }
    Ok(())
}

/// History lives in $HOME/.cache/cliphist, the same file the shell version used
fn history_path() -> io::Result<PathBuf> {
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".cache").join("cliphist"))
}

/// Creates the history file if it is missing, and tells the user when it does
fn ensure_history(path: &Path) -> io::Result<()> {
    if !path.exists() {
        notify(&format!("Creating {}", path.display()));
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        OpenOptions::new().create(true).append(true).open(path)?;
    }
    Ok(())
}

/// Adds `clip` to the history unless it is empty or already there.
/// Returns the notification to show, or None if there was nothing to record.
fn record(path: &Path, clip: &str) -> io::Result<Option<String>> {
    let clip = clip.trim_end_matches('\n');
    if clip.is_empty() {
        return Ok(None);
    }
    let entry = clip.replace('\n', PLACEHOLDER);
    let existing = fs::read_to_string(path).unwrap_or_default();
    if !existing.lines().any(|line| line == entry) {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{entry}")?;
    }
    Ok(Some(format!("\"{entry}\"")))
}

/// Shows the history (newest first) in fleximenu, and copies the chosen entry
fn select(path: &Path) -> io::Result<Option<String>> {
    let history = fs::read_to_string(path).unwrap_or_default();
    let input: String = history
        .lines()
        .rev()
        .map(|line| format!("{line}\n"))
        .collect();

    let mut child = Command::new("fleximenu")
        .args(["-c", "-l", "5", "-i", "-p", "Clipboard history:"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    // fleximenu may exit before reading everything, which is fine
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let output = child.wait_with_output()?;

    let selection = String::from_utf8_lossy(&output.stdout);
    let selection = selection.trim_end_matches('\n');
    if selection.is_empty() {
        return Ok(None);
    }
    set_clipboard(&selection.replace(PLACEHOLDER, "\n"))?;
    Ok(Some("Copied to clipboard!".to_string()))
}

/// Waits for the clipboard's owner to change (XFixes), and records each new
/// clipboard contents. Runs until the X connection ends.
fn watch(path: &Path) -> io::Result<()> {
    ensure_history(path)?;
    unsafe {
        let dpy = XOpenDisplay(null());
        if dpy.is_null() {
            return Err(io::Error::other("cannot open the X display"));
        }
        let root = XDefaultRootWindow(dpy);
        // XFixes events are selected on a window of our own, not the root
        let window = XCreateSimpleWindow(dpy, root, 0, 0, 1, 1, 0, 0, 0);
        let name = CString::new("CLIPBOARD").expect("no NUL in CLIPBOARD");
        let clipboard = XInternAtom(dpy, name.as_ptr(), False);

        let (mut event_base, mut error_base): (c_int, c_int) = (0, 0);
        if XFixesQueryExtension(dpy, &mut event_base, &mut error_base) == 0 {
            xlib::XCloseDisplay(dpy);
            return Err(io::Error::other("the X server has no XFixes extension"));
        }
        XFixesSelectSelectionInput(dpy, window, clipboard, SET_SELECTION_OWNER_NOTIFY_MASK);

        loop {
            let mut event: XEvent = std::mem::zeroed();
            XNextEvent(dpy, &mut event);
            if event.get_type() == event_base + SELECTION_NOTIFY {
                // the owner may have cleared the clipboard, which is nothing to record
                let text = xclip_output(&["-o", "-selection", "clipboard"]).unwrap_or_default();
                if let Err(err) = record(path, &text) {
                    eprintln!("fleximenu_clip: could not write history: {err}");
                }
            }
        }
    }
}

/// Runs `xclip` with the given arguments and returns what it printed
fn xclip_output(args: &[&str]) -> io::Result<String> {
    let output = Command::new("xclip")
        .args(args)
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("xclip failed to read the selection"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Puts `text` on the clipboard. xclip forks to keep serving it, so the parent exits quickly
fn set_clipboard(text: &str) -> io::Result<()> {
    let mut child = Command::new("xclip")
        .args(["-i", "-selection", "clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    if !child.wait()?.success() {
        return Err(io::Error::other("xclip failed to set the clipboard"));
    }
    Ok(())
}

/// Desktop notifications are best effort, so failures are ignored
fn notify(message: &str) {
    let _ = notify_rust::Notification::new()
        .appname("fleximenu_clip")
        .summary(message)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_history(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!("fleximenu_clip_{}_{}", std::process::id(), name));
        let _ = fs::remove_file(&path);
        path
    }

    #[test]
    fn multiline_clip_is_stored_on_one_line() {
        let path = temp_history("multiline");
        let notification = record(&path, "one\ntwo\n").unwrap();
        assert_eq!(notification.as_deref(), Some("\"one<NEWLINE>two\""));
        assert_eq!(fs::read_to_string(&path).unwrap(), "one<NEWLINE>two\n");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn duplicate_entries_are_not_added_twice() {
        let path = temp_history("duplicate");
        record(&path, "same").unwrap();
        record(&path, "same").unwrap();
        record(&path, "other").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "same\nother\n");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn empty_clip_is_not_recorded() {
        let path = temp_history("empty");
        assert_eq!(record(&path, "\n").unwrap(), None);
        assert!(!path.exists());
    }
}
