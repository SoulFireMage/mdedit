//! File open/save helpers plus GTK file/alert dialogs.

use std::path::Path;

use gtk4::gio;
use gtk4::prelude::*;

/// Read a text file as UTF-8, normalising CRLF/CR to LF.
pub fn read_text_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|e| format!("{} is not valid UTF-8: {e}", path.display()))?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Write text as UTF-8 with LF line endings, surfacing any IO error.
pub fn write_text_file(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text.as_bytes())
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))
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