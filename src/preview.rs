//! WebKitGTK 6.0 preview pane.
//!
//! A single static HTML shell is loaded once per document; subsequent
//! markdown renders are injected into its `#content` container via
//! `evaluate_javascript`, preserving the scroll position.
//!
//! The preview keeps the latest content, theme and scroll target itself and
//! re-applies all of it every time a shell load finishes, so nothing sent
//! while a load is in flight can be lost (including when one load cancels
//! another).
//!
//! Link clicks never navigate the preview: in-page anchors are followed,
//! everything else is handed to the desktop's default handler.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::{LoadEvent, NavigationPolicyDecision, NavigationType, PolicyDecisionType};

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
// Scroll so the block rendered from source line `line` is at the top,
// interpolating between the nearest blocks either side.
function __mdeditScrollToLine(line, atEnd) {{
  var root = document.scrollingElement || document.documentElement;
  if (atEnd) {{ window.scrollTo(0, root.scrollHeight); return; }}
  if (line <= 1) {{ window.scrollTo(0, 0); return; }}
  var els = document.querySelectorAll('#content [data-sourcepos]');
  var prev = null, prevLine = 0, next = null, nextLine = 0;
  for (var i = 0; i < els.length; i++) {{
    var start = parseInt(els[i].getAttribute('data-sourcepos'), 10);
    if (start <= line) {{
      if (start > prevLine) {{ prev = els[i]; prevLine = start; }}
    }} else {{ next = els[i]; nextLine = start; break; }}
  }}
  function top(el) {{ return el.getBoundingClientRect().top + window.scrollY; }}
  var y = prev ? top(prev) : 0;
  if (next) {{
    var from = prev ? prevLine : 1;
    y += (top(next) - y) * (line - from) / (nextLine - from);
  }}
  window.scrollTo(0, Math.max(0, y - 8));
}}
</script>
</head>
<body><div id="content" class="markdown-body"></div></body>
</html>"#
    )
}

/// Everything the shell should currently be showing.
struct Shown {
    /// Latest rendered HTML fragment.
    html: RefCell<String>,
    dark: Cell<bool>,
    /// Last scroll target: (top source line, editor scrolled to the end).
    scroll: Cell<(i32, bool)>,
    /// Whether the current shell has finished loading.
    ready: Cell<bool>,
}

/// The live preview: a WebView plus the state it should display.
#[derive(Clone)]
pub struct Preview {
    pub webview: webkit6::WebView,
    shown: Rc<Shown>,
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

        let shown = Rc::new(Shown {
            html: RefCell::new(String::new()),
            dark: Cell::new(dark),
            scroll: Cell::new((1, false)),
            ready: Cell::new(false),
        });

        {
            let shown = shown.clone();
            webview.connect_load_changed(move |webview, event| {
                if event == LoadEvent::Finished {
                    shown.ready.set(true);
                    let (line, at_end) = shown.scroll.get();
                    let script = format!(
                        "{}{}{}",
                        content_script(&shown.html.borrow()),
                        theme_script(shown.dark.get()),
                        scroll_script(line, at_end),
                    );
                    run_script(webview, &script);
                }
            });
        }

        webview.connect_decide_policy(|webview, decision, decision_type| {
            let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else {
                return false;
            };
            let Some(mut action) = nav.navigation_action() else {
                return false;
            };
            let new_window = decision_type == PolicyDecisionType::NewWindowAction;
            // Our own `load_html` calls arrive as `Other`; let those through.
            if !new_window && action.navigation_type() != NavigationType::LinkClicked {
                return false;
            }
            let Some(uri) = action.request().and_then(|r| r.uri()) else {
                decision.ignore();
                return true;
            };
            if !new_window && same_document(&uri, webview.uri().as_deref()) {
                return false;
            }
            decision.ignore();
            open_externally(webview, &uri);
            true
        });

        let preview = Self { webview, shown };
        preview.reload(None);
        preview
    }

    /// Reload the HTML shell, optionally with a base URI so relative assets
    /// (images) resolve against the document's directory. Current content
    /// and theme are re-applied once the load finishes.
    pub fn reload(&self, base_uri: Option<String>) {
        self.shown.ready.set(false);
        let shell = build_shell(self.shown.dark.get());
        self.webview.load_html(&shell, base_uri.as_deref());
    }

    /// Push a rendered HTML fragment into the preview.
    pub fn render_fragment(&self, fragment: &str) {
        *self.shown.html.borrow_mut() = fragment.to_string();
        self.eval(&content_script(fragment));
    }

    /// Toggle the preview theme without a full reload.
    pub fn set_dark(&self, dark: bool) {
        self.shown.dark.set(dark);
        self.eval(&theme_script(dark));
    }

    /// Scroll so the block from source line `line` is at the top (or to the
    /// very end when the editor is scrolled to its end).
    pub fn scroll_to_line(&self, line: i32, at_end: bool) {
        self.shown.scroll.set((line, at_end));
        self.eval(&scroll_script(line, at_end));
    }

    /// Run `script` now if the shell is ready; otherwise the load-finished
    /// handler will apply the stored state.
    fn eval(&self, script: &str) {
        if self.shown.ready.get() {
            run_script(&self.webview, script);
        }
    }
}

fn content_script(fragment: &str) -> String {
    let json = serde_json::to_string(fragment).unwrap_or_else(|_| "\"\"".to_string());
    format!("__mdeditSetContent({json});")
}

fn theme_script(dark: bool) -> String {
    format!("__mdeditSetTheme({dark});")
}

fn scroll_script(line: i32, at_end: bool) -> String {
    format!("__mdeditScrollToLine({line}, {at_end});")
}

fn run_script(webview: &webkit6::WebView, script: &str) {
    webview.evaluate_javascript(
        script,
        None,
        None,
        None::<&gtk4::gio::Cancellable>,
        |_res| {},
    );
}

/// Whether `target` only differs from the current page URI by its fragment.
fn same_document(target: &str, current: Option<&str>) -> bool {
    let strip = |u: &str| u.split('#').next().unwrap_or("").to_string();
    current.is_some_and(|c| strip(c) == strip(target))
}

/// Hand a link to the desktop's default handler (browser, file manager, …).
fn open_externally(webview: &webkit6::WebView, uri: &str) {
    let parent = webview
        .root()
        .and_then(|r| r.downcast::<gtk4::Window>().ok());
    gtk4::UriLauncher::new(uri).launch(parent.as_ref(), None::<&gtk4::gio::Cancellable>, |_res| {});
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
