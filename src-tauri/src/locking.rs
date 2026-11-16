//! Configuration-locked customer builds.
//!
//! A customer build is a copy of the running executable with the active widget
//! layout appended in a small trailer. On startup the app looks for that
//! trailer first; when it is present the layout is read from the executable
//! itself and every command that could replace it is refused.
//!
//! Keeping the layout in the executable means the customer receives one file
//! and the app never has to trust (or even look at) a `widgets.toml` on disk.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// End-of-file marker. Chosen to be long and specific enough that it will not
/// occur at the end of a normal application binary.
pub const LOCK_MAGIC: &[u8] = b"\n<<<BIASED_LOCKED_CONFIG_V1>>>\n";

/// Bytes added after the layout: its little-endian length and the magic.
const TRAILER_BYTES: usize = 8 + LOCK_MAGIC.len();

/// Build the trailer appended to a customer executable.
fn trailer(toml: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(toml.len() + TRAILER_BYTES);
    out.extend_from_slice(toml.as_bytes());
    out.extend_from_slice(&(toml.len() as u64).to_le_bytes());
    out.extend_from_slice(LOCK_MAGIC);
    out
}

/// Return the embedded layout, or `None` when `bytes` is an ordinary build.
///
/// `Some(Err(_))` means the trailer is present but malformed; the caller should
/// still treat the executable as locked rather than silently falling back to an
/// editable configuration.
#[cfg(test)]
pub fn extract(bytes: &[u8]) -> Option<Result<String, String>> {
    if bytes.len() < TRAILER_BYTES + 1 || !bytes.ends_with(LOCK_MAGIC) {
        return None;
    }

    let length_at = bytes.len() - TRAILER_BYTES;
    let mut length = [0u8; 8];
    length.copy_from_slice(&bytes[length_at..length_at + 8]);
    let length = u64::from_le_bytes(length);

    if length == 0 {
        return Some(Err("embedded configuration is empty".into()));
    }

    let Some(start) = (length_at as u64).checked_sub(length) else {
        return Some(Err("embedded configuration is truncated".into()));
    };

    match String::from_utf8(bytes[start as usize..length_at].to_vec()) {
        Ok(text) => Some(Ok(text)),
        Err(_) => Some(Err("embedded configuration is not valid UTF-8".into())),
    }
}

/// Read the trailer from a file without loading the whole executable.
pub fn read_from(path: &Path) -> Option<Result<String, String>> {
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();

    if size < (TRAILER_BYTES + 1) as u64 {
        return None;
    }

    let mut magic = vec![0u8; LOCK_MAGIC.len()];
    if file
        .seek(SeekFrom::End(-(LOCK_MAGIC.len() as i64)))
        .is_err()
        || file.read_exact(&mut magic).is_err()
    {
        return None;
    }
    if magic != LOCK_MAGIC {
        return None;
    }

    // The trailer is present, so from here on a read error is a broken lock,
    // never a reason to silently fall back to an editable configuration.
    Some(read_trailer(&mut file, size))
}

fn read_trailer(file: &mut File, size: u64) -> Result<String, String> {
    let read_error = |action: &str, error: std::io::Error| {
        format!("cannot read the embedded configuration ({action}): {error}")
    };

    file.seek(SeekFrom::End(-(TRAILER_BYTES as i64)))
        .map_err(|error| read_error("footer", error))?;
    let mut length = [0u8; 8];
    file.read_exact(&mut length)
        .map_err(|error| read_error("length", error))?;
    let length = u64::from_le_bytes(length);

    if length == 0 {
        return Err("embedded configuration is empty".into());
    }

    let payload_end = size - TRAILER_BYTES as u64;
    let start = payload_end
        .checked_sub(length)
        .ok_or_else(|| "embedded configuration is truncated".to_string())?;
    let length =
        usize::try_from(length).map_err(|_| "embedded configuration is too large".to_string())?;

    file.seek(SeekFrom::Start(start))
        .map_err(|error| read_error("payload offset", error))?;
    let mut text = vec![0u8; length];
    file.read_exact(&mut text)
        .map_err(|error| read_error("payload", error))?;

    String::from_utf8(text).map_err(|_| "embedded configuration is not valid UTF-8".to_string())
}

/// The file the running app should read its own bytes from.
///
/// AppImages are mounted to a temporary directory: `current_exe()` points at
/// the unpacked binary, while `$APPIMAGE` points at the file the customer
/// actually starts. The lock trailer lives on the latter.
pub fn executable_path() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(path) = std::env::var_os("APPIMAGE") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    std::env::current_exe().ok()
}

/// Copy `source` to `target` and append `toml` as a lock trailer.
///
/// The copy is written through a sibling `.part` file so an interrupted export
/// never leaves a half-written executable at the destination. Returns the final
/// file size.
pub fn write_copy(source: &Path, target: &Path, toml: &str) -> Result<u64, String> {
    let same_file = match (fs::canonicalize(source), fs::canonicalize(target)) {
        (Ok(source), Ok(target)) => source == target,
        _ => false,
    };
    if same_file {
        return Err(
            "Choose a different destination; the running executable cannot be overwritten.".into(),
        );
    }

    let mut bytes =
        fs::read(source).map_err(|error| format!("Cannot read {}: {error}", source.display()))?;
    bytes.extend_from_slice(&trailer(toml));

    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .ok_or_else(|| "Choose a file name for the customer build.".to_string())?;
    let temporary = parent.join(format!(".{}.part", name.to_string_lossy()));

    fs::write(&temporary, &bytes)
        .map_err(|error| format!("Cannot write {}: {error}", temporary.display()))?;

    // Keep the executable bit (and any other permissions) from the source.
    if let Ok(metadata) = fs::metadata(source) {
        let _ = fs::set_permissions(&temporary, metadata.permissions());
    }

    if target.exists() {
        fs::remove_file(target)
            .map_err(|error| format!("Cannot replace {}: {error}", target.display()))?;
    }

    fs::rename(&temporary, target).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("Cannot move the export into {}: {error}", target.display())
    })?;

    Ok(bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_an_embedded_layout() {
        let mut executable = b"pretend this is an executable".to_vec();
        executable.extend_from_slice(&trailer("title = \"Customer\""));

        assert_eq!(
            extract(&executable).unwrap().unwrap(),
            "title = \"Customer\""
        );
    }

    #[test]
    fn ignores_an_ordinary_binary() {
        assert!(extract(b"pretend this is an executable").is_none());
    }

    #[test]
    fn reports_a_truncated_trailer() {
        let mut bytes = trailer("title = \"Customer\"");
        bytes.remove(0);
        assert!(extract(&bytes).unwrap().is_err());
    }

    #[test]
    fn reads_a_trailer_from_disk() {
        let path = std::env::temp_dir().join(format!(
            "biased-lock-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut bytes = b"pretend this is an executable".to_vec();
        bytes.extend_from_slice(&trailer("title = \"From disk\""));
        fs::write(&path, &bytes).unwrap();

        assert_eq!(read_from(&path).unwrap().unwrap(), "title = \"From disk\"");

        let _ = fs::remove_file(path);
    }
}
