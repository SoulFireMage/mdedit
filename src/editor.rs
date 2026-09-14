//! GtkSourceView 5 source editor pane.

use gtk4::prelude::*;
use sourceview5::prelude::*;

/// The markdown source editor: a `sourceview5::View` plus its `Buffer`.
#[derive(Clone)]
pub struct Editor {
    pub view: sourceview5::View,
    pub buffer: sourceview5::Buffer,
}

impl Editor {
    pub fn new() -> Self {
        let manager = sourceview5::LanguageManager::default();
        let buffer = match manager.language("markdown") {
            Some(lang) => sourceview5::Buffer::with_language(&lang),
            None => sourceview5::Buffer::new(None),
        };

        let view = sourceview5::View::with_buffer(&buffer);
        view.set_show_line_numbers(true);
        view.set_highlight_current_line(true);
        view.set_auto_indent(true);
        view.set_insert_spaces_instead_of_tabs(true);
        view.set_tab_width(4);
        view.set_monospace(true);
        view.set_wrap_mode(gtk4::WrapMode::WordChar);
        view.set_left_margin(6);
        view.set_right_margin(6);
        view.set_top_margin(6);
        view.set_bottom_margin(6);
        view.set_vexpand(true);
        view.set_hexpand(true);

        Self { view, buffer }
    }

    /// Full contents of the buffer as a `String`.
    pub fn text(&self) -> String {
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.text(&start, &end, false).to_string()
    }

    /// Replace the whole buffer contents.
    pub fn set_text(&self, text: &str) {
        self.buffer.set_text(text);
    }

    /// 1-based (line, column) of the cursor.
    pub fn cursor_line_col(&self) -> (i32, i32) {
        let offset = self.buffer.cursor_position();
        let iter = self.buffer.iter_at_offset(offset);
        (iter.line() + 1, iter.line_offset() + 1)
    }

    /// Whitespace-separated word count.
    pub fn word_count(&self) -> usize {
        self.text().split_whitespace().count()
    }

    /// Move the cursor to the very start of the document.
    pub fn place_cursor_start(&self) {
        let mut iter = self.buffer.start_iter();
        self.buffer.place_cursor(&iter);
        let _ = &mut iter;
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}