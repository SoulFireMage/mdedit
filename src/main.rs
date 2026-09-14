//! mdedit — side-by-side Markdown editor with live HTML preview.
//!
//! CLI is parsed *before* any GTK initialisation so that `--version` and the
//! headless `--render` mode work with no `DISPLAY`.

mod app;
mod editor;
mod file_ops;
mod markdown;
mod preview;
mod window;

use gtk4::gio::prelude::*;

use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    // BUG B FIX: Disable WebKit sandbox if AppArmor restricts unprivileged user namespaces.
    // This must run before any GTK/WebKit init.
    if let Ok(content) = std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns") {
        if content.trim() == "1" {
            std::env::set_var("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS", "1");
        }
    }

    // Silence known-benign GTK4 theme-parser errors. The system GTK4 theme
    // (Greybird's gtk-4.0/gtk.css) contains GTK3-only syntax, so GTK logs a
    // warning per invalid declaration. GTK4 logs via structured logging, so we
    // filter at the writer level and forward every other entry unchanged.
    gtk4::glib::log_set_writer_func(|level, fields| {
        let mut domain: Option<&str> = None;
        let mut message: Option<&str> = None;
        for field in fields {
            match field.key() {
                "GLIB_DOMAIN" => domain = field.value_str(),
                "MESSAGE" => message = field.value_str(),
                _ => {}
            }
        }
        let benign = matches!(domain, Some("Gtk"))
            && message.is_some_and(|m| {
                m.contains("Theme parser error") || m.contains("Unknown key gtk-modules")
            });
        if benign {
            gtk4::glib::LogWriterOutput::Handled
        } else {
            gtk4::glib::log_writer_default(level, fields)
        }
    });

    // The Intel Vulkan driver emits "FINISHME" warnings while WebKit's DMA-BUF
    // renderer probes formats. Use the non-DMA-BUF path (harmless on X11) unless
    // the caller explicitly chose otherwise.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let args: Vec<String> = std::env::args().collect();
    // args[0] is the program name.
    match args.get(1).map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("mdedit {VERSION}");
            ExitCode::SUCCESS
        }
        Some("--render") => match args.get(2) {
            Some(path) => render_file(path),
            None => {
                eprintln!("mdedit: --render requires a <file.md> argument");
                ExitCode::from(2)
            }
        },
        Some("--help") | Some("-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        // Normal GUI launch (optional file path argument).
        _ => {
            let path = args
                .get(1)
                .filter(|a| !a.starts_with('-'))
                .map(PathBuf::from);

            let app = gtk4::Application::new(
                Some("org.richard.mdedit"),
                gtk4::gio::ApplicationFlags::HANDLES_OPEN,
            );
            app.connect_activate(move |app| {
                window::build_ui(app, path.clone());
            });
            app.connect_open(move |_app, files, _n_files| {
                window::build_ui(&_app, files.first().and_then(|f| f.path()));
            });
            let code = app.run();
            ExitCode::from(code.value() as u8)
        }
    }
}

fn print_help() {
    println!(
        "mdedit {VERSION} — side-by-side Markdown editor with live preview\n\
         \n\
         USAGE:\n\
         \x20   mdedit [FILE.md]        Launch the GUI, optionally opening FILE.md\n\
         \x20   mdedit --render FILE.md Render FILE.md to an HTML fragment on stdout\n\
         \x20   mdedit --version        Print version and exit\n\
         \x20   mdedit --help           Print this help"
    );
}

/// Headless render: read a file, print the sanitised HTML fragment, exit.
fn render_file(path: &str) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("mdedit: cannot read {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("mdedit: {path} is not valid UTF-8: {e}");
            return ExitCode::from(1);
        }
    };
    // Normalise CRLF -> LF for consistent rendering.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    print!("{}", markdown::render_markdown(&text));
    ExitCode::SUCCESS
}
