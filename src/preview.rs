//! WebKitGTK 6.0 preview pane.
//!
//! A single static HTML shell is loaded once; subsequent markdown renders are
//! injected into its `#content` container via `evaluate_javascript`, preserving
//! the scroll position. If the shell is not ready yet the update is queued and
//! flushed on the `load-changed` `Finished` event.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::LoadEvent;

/// CSS shipped with the binary.
const PREVIEW_CSS: &str = include_str!("../resources/preview.css");

/// Build the static HTML shell with the current theme applied.
fn build_shell(dark: bool) -> String {
    let theme_class = if dark { "dark" } else { "light" };
    let css = PREVIEW_CSS.replace("</style>", "");
    format!(
        r#"<!DOCTYPE html>
<html class="{theme_class}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>{css}</style>
<script>
function __mdeditSetContent(html) {{
  var y = window.scrollY || window.pageYOffset || 0;
  var c = document.getElementById('content');
  if (c) {{ c.innerHTML = html; }}
  window.scrollTo(0, y);
}}
function __mdeditSetTheme(dark) {{
  var root = document.documentElement;
  root.classList.toggle('dark', !!dark);
  root.classList.toggle('light', !dark);
}}
</script>
</head>
<body><div id="content" class="markdown-body"></div></body>
</html>"#
    )
}

/// The live preview: a WebView plus shell-readiness bookkeeping.
#[derive(Clone)]
pub struct Preview {
    pub webview: webkit6::WebView,
    ready: Rc<Cell<bool>>,
    pending: Rc<RefCell<Option<String>>>,
}

impl Preview {
    pub fn new(dark: bool) -> Self {
        let webview = webkit6::WebView::new();
        if let Some(settings) = webkit6::prelude::WebViewExt::settings(&webview) {
            // JavaScript is required for our shell's content-injection helper.
            // User raw HTML is already escaped by comrak, so nothing
            // user-controlled is ever executed.
            settings.set_enable_javascript(true);
            settings.set_enable_developer_extras(false);
            settings.set_javascript_can_open_windows_automatically(false);
        }

        let ready = Rc::new(Cell::new(false));
        let pending: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        {
            let ready = ready.clone();
            let pending = pending.clone();
            let webview_cb = webview.clone();
            webview.connect_load_changed(move |_, event| {
                if event == LoadEvent::Finished {
                    ready.set(true);
                    if let Some(script) = pending.borrow_mut().take() {
                        webview_cb.evaluate_javascript(
                            &script,
                            None,
                            None,
                            None::<&gtk4::gio::Cancellable>,
                            |_res| {},
                        );
                    }
                }
            });
        }

        let preview = Self {
            webview,
            ready,
            pending,
        };
        preview.reload(None, dark);
        preview
    }

    /// Reload the HTML shell, optionally with a base URI so relative assets
    /// (images) resolve against the document's directory.
    pub fn reload(&self, base_uri: Option<String>, dark: bool) {
        self.ready.set(false);
        let shell = build_shell(dark);
        self.webview.load_html(&shell, base_uri.as_deref());
    }

    /// Push a rendered HTML fragment into the preview.
    pub fn render_fragment(&self, fragment: &str) {
        let json = serde_json::to_string(fragment).unwrap_or_else(|_| "\"\"".to_string());
        self.eval(&format!("__mdeditSetContent({json});"));
    }

    /// Toggle the preview theme without a full reload.
    pub fn set_dark(&self, dark: bool) {
        self.eval(&format!("__mdeditSetTheme({});", dark));
    }

    fn eval(&self, script: &str) {
        if self.ready.get() {
            self.webview.evaluate_javascript(
                script,
                None,
                None,
                None::<&gtk4::gio::Cancellable>,
                |_res| {},
            );
        } else {
            *self.pending.borrow_mut() = Some(script.to_string());
        }
    }
}

/// Build a `file:///.../dir/` base URI from a document path's parent directory.
pub fn base_uri_for_path(path: &Path) -> String {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => Path::new(".").to_path_buf(),
    };
    let mut uri = gtk4::gio::File::for_path(dir).uri().to_string();
    if !uri.ends_with('/') {
        uri.push('/');
    }
    uri
}