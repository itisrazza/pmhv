//! Screen captures shown on machine cards: a PNG for graphical consoles, the serial console's
//! last screenful as text otherwise. Kept in the machine's data directory so stopped machines
//! still show what they last displayed.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pmhv_model::{Thumbnail, ThumbnailKind};

const TEXT_LINES: usize = 40;

pub fn image_file(machine_dir: &Path) -> PathBuf {
    machine_dir.join("thumbnail.png")
}

pub fn text_file(machine_dir: &Path) -> PathBuf {
    machine_dir.join("thumbnail.txt")
}

/// The newest capture in a machine's directory.
pub fn current(machine_dir: &Path) -> Option<(Thumbnail, PathBuf)> {
    let candidates = [
        (ThumbnailKind::Image, image_file(machine_dir)),
        (ThumbnailKind::Text, text_file(machine_dir)),
    ];
    candidates
        .into_iter()
        .filter_map(|(kind, path)| {
            let captured_at = modified_millis(&path)?;
            Some((Thumbnail { kind, captured_at }, path))
        })
        .max_by_key(|(thumbnail, _)| thumbnail.captured_at)
}

fn modified_millis(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let millis = modified.duration_since(UNIX_EPOCH).ok()?.as_millis();
    u64::try_from(millis).ok()
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Writes atomically, so a reader never sees a half-written capture.
pub fn write_text(machine_dir: &Path, scrollback: &str) -> io::Result<()> {
    let path = text_file(machine_dir);
    let temporary = path.with_extension("txt.tmp");
    std::fs::write(&temporary, screenful(scrollback))?;
    std::fs::rename(&temporary, &path)?;
    // Only the newest capture is shown, but a stale PNG would win on age if the console changed.
    remove_if_present(&image_file(machine_dir))
}

pub fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The last lines of terminal output with escape sequences and carriage returns removed.
pub fn screenful(scrollback: &str) -> String {
    let plain = strip_escapes(scrollback);
    let lines: Vec<&str> = plain
        .split('\n')
        .map(|line| {
            line.rsplit('\r')
                .find(|part| !part.is_empty())
                .unwrap_or("")
        })
        .collect();
    let start = lines.len().saturating_sub(TEXT_LINES);
    lines[start..].join("\n")
}

/// Drops ANSI CSI (`ESC [ … final`) and OSC (`ESC ] … BEL`) sequences and other control bytes.
fn strip_escapes(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\u{1b}' => match chars.next() {
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}'
                            || (next == '\u{1b}' && chars.next_if_eq(&'\\').is_some())
                        {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' | '\r' | '\t' => output.push(character),
            control if control.is_control() => {}
            printable => output.push(printable),
        }
    }
    output
}
