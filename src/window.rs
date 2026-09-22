//! Main application window: layout, actions, and the live-preview pipeline.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, HeaderBar, Label, MenuButton, Orientation, Paned,
    ScrolledWindow,
};

use crate::app::{display_name, AppState, PendingAction};
use crate::editor::Editor;
use crate::file_ops;
use crate::markdown;
use crate::preview::{base_uri_for_path, Preview};
use crate::window_state::WindowState;

/// Debounce delay between edits and preview refresh.
const PREVIEW_DEBOUNCE_MS: u64 = 200;

/// Everything the callbacks need, cheaply cloneable.
#[derive(Clone)]
struct Ui {
    state: Rc<AppState>,
    editor: Editor,
    preview: Preview,
    window: ApplicationWindow,
    title: Label,
    status: Label,
    source_scroll: ScrolledWindow,
    preview_scroll: ScrolledWindow,
    paned: Paned,
}

impl Ui {
    fn update_title(&self) {
        let name = display_name(self.state.path.borrow().as_deref());
        let star = if self.state.modified.get() { " *" } else { "" };
        let text = format!("{name}{star}");
        self.title.set_text(&text);
        self.window.set_title(Some(&format!("{text} — MDEdit")));
    }

    /// Cheap enough for every cursor move: the word count is cached by
    /// `render` rather than recomputed from the whole buffer here.
    fn update_status(&self) {
        let (line, col) = self.editor.cursor_line_col();
        let words = self.state.words.get();
        self.status
            .set_text(&format!("Ln {line}, Col {col}   ·   {words} words"));
    }

    /// Schedule a debounced preview refresh.
    fn schedule_update(&self) {
        self.state.cancel_debounce();
        let ui = self.clone();
        let id = gtk4::glib::timeout_add_local_once(
            Duration::from_millis(PREVIEW_DEBOUNCE_MS),
            move || {
                // BUG C FIX: Clear the stored SourceId before any work.
                // This prevents the panic when cancel_debounce() tries to remove
                // a source that has already fired and been destroyed.
                *ui.state.debounce.borrow_mut() = None;
                ui.render();
            },
        );
        *self.state.debounce.borrow_mut() = Some(id);
    }

    /// Immediately render the current buffer (used after load/save).
    fn refresh_now(&self) {
        self.state.cancel_debounce();
        self.render();
    }

    /// Render the buffer into the preview and refresh the cached word count.
    fn render(&self) {
        let text = self.editor.text();
        self.state.words.set(text.split_whitespace().count());
        self.preview
            .render_fragment(&markdown::render_preview(&text));
        self.update_status();
        self.sync_preview_scroll();
    }

    /// Scroll the preview to match the editor's top visible line.
    fn sync_preview_scroll(&self) {
        let adj = self.source_scroll.vadjustment();
        // Before the editor is first laid out the adjustment is all zeros and
        // `line_at_y` reports the *last* line, so treat that as the top.
        if adj.page_size() <= 0.0 {
            self.preview.scroll_to_line(1, false);
            return;
        }
        // An unscrolled editor is never "at the end", even when the text view
        // is still measuring lines and `upper` barely exceeds `page_size`.
        let at_end = adj.value() > 0.0 && adj.value() >= adj.upper() - adj.page_size() - 1.0;
        self.preview
            .scroll_to_line(self.editor.top_visible_line(), at_end);
    }

    fn open_path(&self, path: PathBuf) {
        match file_ops::read_text_file(&path) {
            Ok(text) => {
                self.state.loading.set(true);
                self.editor.load_text(&text);
                self.editor.place_cursor_start();
                self.state.loading.set(false);

                self.state.disk_mtime.set(file_ops::modified_time(&path));
                *self.state.path.borrow_mut() = Some(path.clone());
                self.state.modified.set(false);
                self.state.pending.borrow_mut().take();

                self.preview.reload(Some(base_uri_for_path(&path)));
                self.refresh_now();
                self.update_title();
            }
            Err(message) => file_ops::show_error(&self.window, &message),
        }
    }

    fn new_doc(&self) {
        self.state.loading.set(true);
        self.editor.load_text("");
        self.state.loading.set(false);
        *self.state.path.borrow_mut() = None;
        self.state.disk_mtime.set(None);
        self.state.modified.set(false);
        self.state.pending.borrow_mut().take();

        self.preview.reload(None);
        self.refresh_now();
        self.update_title();
    }

    fn suggested_name(&self) -> String {
        self.state
            .path
            .borrow()
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled.md".to_string())
    }

    fn choose_open(&self) {
        let dialog = gtk4::FileDialog::builder().title("Open Markdown").build();
        dialog.set_filters(Some(&file_ops::markdown_filters()));
        let ui = self.clone();
        dialog.open(
            Some(&self.window),
            None::<&gio::Cancellable>,
            move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        ui.open_path(path);
                    }
                }
            },
        );
    }

    /// Save the document; with `save_as` forced, always show the file chooser.
    fn do_save(&self, save_as: bool) {
        if !save_as {
            let existing = self.state.path.borrow().clone();
            if let Some(path) = existing {
                self.write_to(path);
                return;
            }
        }

        let dialog = gtk4::FileDialog::builder().title("Save Markdown").build();
        dialog.set_initial_name(Some(&self.suggested_name()));
        dialog.set_filters(Some(&file_ops::markdown_filters()));
        let ui = self.clone();
        dialog.save(
            Some(&self.window),
            None::<&gio::Cancellable>,
            move |result| match result.ok().and_then(|file| file.path()) {
                Some(path) => ui.write_to(path),
                // Save was abandoned, so whatever was waiting on it (New,
                // Open, Quit) must not fire on some later, unrelated save.
                None => {
                    ui.state.pending.borrow_mut().take();
                }
            },
        );
    }

    fn write_to(&self, path: PathBuf) {
        let text = self.editor.text();
        match file_ops::write_text_file(&path, &text) {
            Ok(()) => {
                let path_changed = self.state.path.borrow().as_deref() != Some(path.as_path());
                self.state.disk_mtime.set(file_ops::modified_time(&path));
                *self.state.path.borrow_mut() = Some(path.clone());
                self.state.modified.set(false);

                if path_changed {
                    self.preview.reload(Some(base_uri_for_path(&path)));
                    self.refresh_now();
                }
                self.update_title();

                // Take the action in its own statement: in an `if let` the
                // `RefCell` borrow would live through the block, and
                // `new_doc` borrowing `pending` again would panic.
                let action = self.state.pending.borrow_mut().take();
                if let Some(action) = action {
                    self.execute_pending(action);
                }
            }
            Err(message) => {
                self.state.pending.borrow_mut().take();
                file_ops::show_error(&self.window, &message);
            }
        }
    }

    /// Ask about unsaved changes, then run `action`.
    fn confirm_unsaved(&self, action: PendingAction) {
        let dialog = gtk4::AlertDialog::builder()
            .message("Save changes before continuing?")
            .detail("Your document has unsaved changes.")
            .modal(true)
            .build();
        // macOS-style ordering is not guaranteed; indices: 0 Cancel,
        // 1 Discard, 2 Save (GTK may reorder for the platform).
        dialog.set_buttons(&["Cancel", "Discard", "Save"]);
        dialog.set_cancel_button(0);
        dialog.set_default_button(2);

        let ui = self.clone();
        dialog.choose(
            Some(&self.window),
            None::<&gio::Cancellable>,
            move |result| match result {
                Ok(0) => {}
                Ok(1) => ui.execute_pending(action),
                Ok(2) => {
                    *ui.state.pending.borrow_mut() = Some(action);
                    ui.do_save(false);
                }
                _ => {}
            },
        );
    }

    fn execute_pending(&self, action: PendingAction) {
        match action {
            PendingAction::New => self.new_doc(),
            PendingAction::Open => self.choose_open(),
            PendingAction::Quit => self.window.destroy(),
        }
    }

    fn request_new(&self) {
        if self.state.modified.get() {
            self.confirm_unsaved(PendingAction::New);
        } else {
            self.new_doc();
        }
    }

    fn request_open(&self) {
        if self.state.modified.get() {
            self.confirm_unsaved(PendingAction::Open);
        } else {
            self.choose_open();
        }
    }

    fn toggle_preview(&self) {
        let visible = self.preview_scroll.is_visible();
        self.preview_scroll.set_visible(!visible);
    }

    /// If another program changed the file since we loaded or saved it,
    /// offer to reload it.
    fn check_disk_changes(&self) {
        if self.state.disk_prompt_open.get() {
            return;
        }
        let Some(path) = self.state.path.borrow().clone() else {
            return;
        };
        let on_disk = file_ops::modified_time(&path);
        // A deleted file is left alone: saving will simply recreate it.
        if on_disk.is_none() || on_disk == self.state.disk_mtime.get() {
            return;
        }

        let detail = if self.state.modified.get() {
            "Another program modified this file. Reloading will discard your unsaved edits."
        } else {
            "Another program modified this file."
        };
        let dialog = gtk4::AlertDialog::builder()
            .message(format!("“{}” changed on disk", display_name(Some(&path))))
            .detail(detail)
            .modal(true)
            .build();
        dialog.set_buttons(&["Keep Mine", "Reload"]);
        dialog.set_cancel_button(0);
        dialog.set_default_button(1);

        self.state.disk_prompt_open.set(true);
        let ui = self.clone();
        dialog.choose(
            Some(&self.window),
            None::<&gio::Cancellable>,
            move |result| {
                ui.state.disk_prompt_open.set(false);
                // The prompt may have outlived the document it was about.
                if ui.state.path.borrow().as_deref() != Some(path.as_path()) {
                    return;
                }
                match result {
                    Ok(1) => ui.open_path(path),
                    // Keep ours; don't ask again about this same change.
                    _ => ui.state.disk_mtime.set(on_disk),
                }
            },
        );
    }

    fn save_window_state(&self) {
        let (width, height) = self.window.default_size();
        WindowState {
            width,
            height,
            maximized: self.window.is_maximized(),
            split: self.paned.position(),
        }
        .save();
    }

    fn set_dark(&self, dark: bool) {
        if self.state.dark.get() == dark {
            return;
        }
        self.state.dark.set(dark);
        self.preview.set_dark(dark);
    }
}

/// Detect the current GTK colour scheme preference.
fn detect_dark() -> bool {
    if let Some(settings) = gtk4::Settings::default() {
        if settings.is_gtk_application_prefer_dark_theme() {
            return true;
        }
        if let Some(name) = settings.gtk_theme_name() {
            if name.to_lowercase().contains("-dark") {
                return true;
            }
        }
    }
    false
}

/// Build the main window and wire up all behaviour.
pub fn build_ui(app: &gtk4::Application, startup_file: Option<PathBuf>) {
    let dark = detect_dark();
    let state = AppState::new(dark);
    let editor = Editor::new();
    let preview = Preview::new(dark);
    let saved = WindowState::load();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("MDEdit")
        .default_width(saved.width)
        .default_height(saved.height)
        .maximized(saved.maximized)
        .build();

    // --- layout -----------------------------------------------------------
    let source_scroll = ScrolledWindow::builder()
        .child(&editor.view)
        .vexpand(true)
        .hexpand(true)
        .build();

    let preview_scroll = ScrolledWindow::builder()
        .child(&preview.webview)
        .vexpand(true)
        .hexpand(true)
        .build();

    let paned = Paned::new(Orientation::Horizontal);
    paned.set_start_child(Some(&source_scroll));
    paned.set_end_child(Some(&preview_scroll));
    paned.set_resize_start_child(true);
    paned.set_shrink_start_child(false);
    paned.set_resize_end_child(true);
    paned.set_shrink_end_child(false);
    paned.set_position(saved.split);
    paned.set_vexpand(true);

    let status = Label::new(Some("Ln 1, Col 1   ·   0 words"));
    status.set_xalign(0.0);
    status.set_margin_top(4);
    status.set_margin_bottom(4);
    status.set_margin_start(8);
    status.set_margin_end(8);
    status.add_css_class("dim-label");

    // --- header bar -------------------------------------------------------
    let title = Label::new(Some("Untitled"));
    let header = HeaderBar::new();
    header.set_title_widget(Some(&title));

    let open_btn = gtk4::Button::from_icon_name("document-open-symbolic");
    open_btn.set_tooltip_text(Some("Open (Ctrl+O)"));
    let save_btn = gtk4::Button::from_icon_name("document-save-symbolic");
    save_btn.set_tooltip_text(Some("Save (Ctrl+S)"));
    header.pack_start(&open_btn);
    header.pack_start(&save_btn);

    let menu = gio::Menu::new();
    menu.append(Some("New"), Some("win.new"));
    menu.append(Some("Open…"), Some("win.open"));
    menu.append(Some("Save"), Some("win.save"));
    menu.append(Some("Save As…"), Some("win.save-as"));
    menu.append(Some("Toggle Preview"), Some("win.toggle-preview"));
    menu.append(Some("Quit"), Some("win.quit"));
    let menu_btn = MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .build();
    menu_btn.set_tooltip_text(Some("Menu"));
    header.pack_end(&menu_btn);
    window.set_titlebar(Some(&header));

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.append(&paned);
    root.append(&status);
    window.set_child(Some(&root));

    let ui = Ui {
        state: state.clone(),
        editor: editor.clone(),
        preview: preview.clone(),
        window: window.clone(),
        title,
        status,
        source_scroll: source_scroll.clone(),
        preview_scroll,
        paned,
    };

    // --- actions ----------------------------------------------------------
    add_action(&window, "new", {
        let ui = ui.clone();
        move |_, _| ui.request_new()
    });
    add_action(&window, "open", {
        let ui = ui.clone();
        move |_, _| ui.request_open()
    });
    add_action(&window, "save", {
        let ui = ui.clone();
        move |_, _| ui.do_save(false)
    });
    add_action(&window, "save-as", {
        let ui = ui.clone();
        move |_, _| ui.do_save(true)
    });
    add_action(&window, "toggle-preview", {
        let ui = ui.clone();
        move |_, _| ui.toggle_preview()
    });
    add_action(&window, "quit", {
        let ui = ui.clone();
        move |_, _| {
            ui.window.close();
        }
    });

    app.set_accels_for_action("win.new", &["<Ctrl>n"]);
    app.set_accels_for_action("win.open", &["<Ctrl>o"]);
    app.set_accels_for_action("win.save", &["<Ctrl>s"]);
    app.set_accels_for_action("win.save-as", &["<Ctrl><Shift>s"]);
    app.set_accels_for_action("win.toggle-preview", &["F9"]);
    app.set_accels_for_action("win.quit", &["<Ctrl>q"]);

    // --- buttons ----------------------------------------------------------
    {
        let ui = ui.clone();
        open_btn.connect_clicked(move |_| ui.request_open());
    }
    {
        let ui = ui.clone();
        save_btn.connect_clicked(move |_| ui.do_save(false));
    }

    // --- editor -> preview ------------------------------------------------
    {
        let ui = ui.clone();
        editor.buffer.connect_changed(move |_| {
            if ui.state.loading.get() {
                return;
            }
            ui.state.modified.set(true);
            ui.update_title();
            ui.schedule_update();
        });
    }
    {
        let ui = ui.clone();
        editor
            .buffer
            .connect_cursor_position_notify(move |_| ui.update_status());
    }

    // --- preview follows the editor's scroll position -------------------
    {
        let ui = ui.clone();
        source_scroll
            .vadjustment()
            .connect_value_changed(move |_| ui.sync_preview_scroll());
    }

    // --- theme changes ----------------------------------------------------
    if let Some(settings) = gtk4::Settings::default() {
        let ui_theme = ui.clone();
        settings.connect_gtk_theme_name_notify(move |_| ui_theme.set_dark(detect_dark()));
        let ui_prefer = ui.clone();
        settings.connect_gtk_application_prefer_dark_theme_notify(move |_| {
            ui_prefer.set_dark(detect_dark())
        });
    }

    // --- unsaved-changes on close ----------------------------------------
    {
        let ui = ui.clone();
        window.connect_close_request(move |_| {
            ui.save_window_state();
            if ui.state.modified.get() {
                ui.confirm_unsaved(PendingAction::Quit);
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
    }

    // --- external edits: check whenever the window regains focus ----------
    {
        let ui = ui.clone();
        window.connect_is_active_notify(move |window| {
            if window.is_active() {
                ui.check_disk_changes();
            }
        });
    }

    ui.update_title();
    ui.update_status();
    window.present();

    if let Some(path) = startup_file {
        ui.open_path(path);
    }
}

/// Register a parameterless `win.<name>` action backed by `handler`.
fn add_action<F>(window: &ApplicationWindow, name: &str, handler: F)
where
    F: Fn(&gio::SimpleAction, Option<&gtk4::glib::Variant>) + 'static,
{
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(handler);
    window.add_action(&action);
}
