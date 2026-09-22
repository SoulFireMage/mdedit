//! Pure markdown -> sanitized HTML fragment rendering.
//!
//! This module has **no GTK dependency** so it can be unit-tested and used by
//! the headless `--render` mode without any display.

use comrak::{markdown_to_html, Options};

/// Build the renderer options: GFM extensions on, raw HTML escaped.
///
/// - Raw HTML is *escaped* (`render.escape = true`) rather than emitted, and
///   `render.r#unsafe` stays false so dangerous links are neutralised.
///   This means user markdown containing `<script>` can never execute.
/// - GFM extensions enabled: tables, task lists, strikethrough, autolinks,
///   footnotes.
pub fn options() -> Options<'static> {
    let mut options = Options::default();

    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;

    options.render.escape = true;
    options.render.r#unsafe = false;

    options
}

/// Render a markdown string to an HTML fragment (no `<html>`/`<body>` shell).
pub fn render_markdown(markdown: &str) -> String {
    markdown_to_html(markdown, &options())
}

/// Render for the live preview: as [`render_markdown`], plus
/// `data-sourcepos` attributes the preview uses to follow the editor's
/// scroll position.
pub fn render_preview(markdown: &str) -> String {
    let mut options = options();
    options.render.sourcepos = true;
    markdown_to_html(markdown, &options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_heading_and_emphasis() {
        let html = render_markdown("# Hello\n\nSome *emphasis* and **strong**.\n");
        assert!(html.contains("<h1>Hello</h1>"), "got: {html}");
        assert!(html.contains("<em>emphasis</em>"), "got: {html}");
        assert!(html.contains("<strong>strong</strong>"), "got: {html}");
    }

    #[test]
    fn renders_gfm_table() {
        let html = render_markdown("| a | b |\n|---|---|\n| c | d |\n");
        assert!(html.contains("<table>"), "got: {html}");
        assert!(html.contains("<th>a</th>"), "got: {html}");
        assert!(html.contains("<td>c</td>"), "got: {html}");
    }

    #[test]
    fn renders_tasklist() {
        let html = render_markdown("* [x] Done\n* [ ] Not done\n");
        assert!(html.contains("type=\"checkbox\""), "got: {html}");
        assert!(html.contains("checked"), "got: {html}");
    }

    #[test]
    fn renders_strikethrough() {
        let html = render_markdown("Hello ~world~ there.\n");
        assert!(html.contains("<del>world</del>"), "got: {html}");
    }

    #[test]
    fn renders_autolink() {
        let html = render_markdown("Visit www.github.com today.\n");
        assert!(
            html.contains("<a href=\"http://www.github.com\">"),
            "got: {html}"
        );
    }

    #[test]
    fn raw_html_is_escaped_not_executed() {
        let html = render_markdown("<script>alert('xss')</script>\n");
        // The tag must be escaped as text, never emitted as real markup.
        assert!(!html.contains("<script>"), "script tag leaked: {html}");
        assert!(html.contains("&lt;script&gt;"), "not escaped: {html}");
    }

    #[test]
    fn javascript_url_is_neutralised() {
        let html = render_markdown("[Dangerous](javascript:alert(1))\n");
        assert!(
            !html.contains("javascript:alert"),
            "dangerous url leaked: {html}"
        );
    }

    #[test]
    fn preview_render_carries_source_lines() {
        let html = render_preview("# Title\n\npara\n");
        assert!(html.contains("data-sourcepos=\"1:1-1:7\""), "got: {html}");
        assert!(html.contains("data-sourcepos=\"3:1-3:4\""), "got: {html}");
        // Plain renders (used by --render) stay attribute-free.
        assert!(!render_markdown("# Title\n").contains("data-sourcepos"));
    }

    #[test]
    fn preview_render_still_escapes_raw_html() {
        let html = render_preview("<script>alert('xss')</script>\n");
        assert!(!html.contains("<script>"), "script tag leaked: {html}");
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert_eq!(render_markdown("").trim(), "");
    }
}
