//! File open/save helpers plus GTK file/alert dialogs.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

use gtk4::gio;
use gtk4::prelude::*;

/// Read a text file as UTF-8, normalising CRLF/CR to LF.
pub fn read_text_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|e| format!("{} is not valid UTF-8: {e}", path.display()))?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Write text as UTF-8 with LF line endings, surfacing any IO error.
///
/// The write is atomic: the text goes to a temporary file in the same
/// directory which is then renamed over the target, so a crash or full disk
/// mid-save never leaves a truncated document. The original file's
/// permissions are kept, and a symlink is written through rather than
/// replaced.
pub fn write_text_file(path: &Path, text: &str) -> Result<(), String> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = target
        .file_name()
        .ok_or_else(|| format!("Cannot write {}: not a file path", path.display()))?;
    let dir = target
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp_name = OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(".mdedit-tmp");
    let tmp = dir.join(tmp_name);

    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        if let Ok(meta) = fs::metadata(&target) {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        fs::rename(&tmp, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

/// Last-modified time of a file, if it exists and the platform reports one.
pub fn modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A filter model offering Markdown files (default) and all files.
pub fn markdown_filters() -> gio::ListStore {
    let store = gio::ListStore::new::<gtk4::FileFilter>();

    let md = gtk4::FileFilter::new();
    md.set_name(Some("Markdown"));
    md.add_pattern("*.md");
    md.add_pattern("*.markdown");
    md.add_mime_type("text/markdown");
    store.append(&md);

    let all = gtk4::FileFilter::new();
    all.set_name(Some("All files"));
    all.add_pattern("*");
    store.append(&all);

    store
}

/// Show a modal error dialog.
pub fn show_error(parent: &impl IsA<gtk4::Window>, message: &str) {
    let dialog = gtk4::AlertDialog::builder()
        .message(message)
        .modal(true)
        .build();
    dialog.set_buttons(&["OK"]);
    dialog.choose(Some(parent), None::<&gio::Cancellable>, |_res| {});
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A fresh, empty scratch directory unique to this test.
    fn scratch_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdedit-test-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_normalises_line_endings() {
        let dir = scratch_dir("crlf");
        let path = dir.join("a.md");
        fs::write(&path, "one\r\ntwo\rthree\n").unwrap();
        assert_eq!(read_text_file(&path).unwrap(), "one\ntwo\nthree\n");
    }

    #[test]
    fn read_rejects_non_utf8() {
        let dir = scratch_dir("utf8");
        let path = dir.join("bad.md");
        fs::write(&path, [0x66, 0x6f, 0xff, 0x6f]).unwrap();
        let err = read_text_file(&path).unwrap_err();
        assert!(err.contains("not valid UTF-8"), "got: {err}");
    }

    #[test]
    fn write_replaces_contents_and_leaves_no_temp_file() {
        let dir = scratch_dir("write");
        let path = dir.join("doc.md");
        fs::write(&path, "old contents that are longer").unwrap();
        write_text_file(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        let entries: Vec<_> = fs::read_dir(&dir).unwrap().collect();
        assert_eq!(entries.len(), 1, "temp file left behind");
    }

    #[cfg(unix)]
    #[test]
    fn write_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch_dir("perms");
        let path = dir.join("doc.md");
        fs::write(&path, "x").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        write_text_file(&path, "y").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn write_goes_through_symlinks() {
        let dir = scratch_dir("symlink");
        let real = dir.join("real.md");
        let link = dir.join("link.md");
        fs::write(&real, "x").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_text_file(&link, "y").unwrap();
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "y");
    }
}
