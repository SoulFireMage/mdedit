//! Shared application state for the editor window.
//!
//! State is shared via `Rc<AppState>` (single-threaded GTK main loop only).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use gtk4::glib::SourceId;

/// An action that should run once an unsaved-changes prompt has been resolved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PendingAction {
    New,
    Open,
    Quit,
}

/// Mutable UI/document state.
pub struct AppState {
    /// Path of the document on disk, or `None` for an untitled buffer.
    pub path: RefCell<Option<PathBuf>>,
    /// Whether the buffer has unsaved edits.
    pub modified: Cell<bool>,
    /// The pending preview-debounce timeout, if any.
    pub debounce: RefCell<Option<SourceId>>,
    /// Whether the UI is currently in dark mode.
    pub dark: Cell<bool>,
    /// Set while programmatically loading text, to suppress `changed` handling.
    pub loading: Cell<bool>,
    /// A queued action awaiting the outcome of the save prompt.
    pub pending: RefCell<Option<PendingAction>>,
    /// Word count as of the last preview render (recomputed on the debounce,
    /// not on every cursor move).
    pub words: Cell<usize>,
    /// Modification time of the file when we last loaded or saved it, used to
    /// detect edits made by other programs.
    pub disk_mtime: Cell<Option<SystemTime>>,
    /// Set while the "changed on disk" prompt is showing, so refocusing the
    /// window does not stack a second one.
    pub disk_prompt_open: Cell<bool>,
}

impl AppState {
    pub fn new(dark: bool) -> Rc<Self> {
        Rc::new(Self {
            path: RefCell::new(None),
            modified: Cell::new(false),
            debounce: RefCell::new(None),
            dark: Cell::new(dark),
            loading: Cell::new(false),
            pending: RefCell::new(None),
            words: Cell::new(0),
            disk_mtime: Cell::new(None),
            disk_prompt_open: Cell::new(false),
        })
    }

    /// Cancel a scheduled preview update, if one is pending.
    pub fn cancel_debounce(&self) {
        if let Some(id) = self.debounce.borrow_mut().take() {
            id.remove();
        }
    }
}

/// Human-readable label for the current document.
pub fn display_name(path: Option<&Path>) -> String {
    match path {
        Some(p) => p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned()),
        None => "Untitled".to_string(),
    }
}
