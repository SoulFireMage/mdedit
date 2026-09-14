# MDEdit

A native GTK4 markdown editor for Linux with a side-by-side live HTML preview.

- **Left pane** — editable Markdown source using [GtkSourceView 5](https://gitlab.gnome.org/GNOME/gtksourceview)
  (Markdown syntax highlighting, line numbers, current-line highlight, monospace,
  4-space tabs).
- **Right pane** — live rendered preview using [WebKitGTK 6.0](https://webkitgtk.org/),
  updated ~200 ms after you stop typing.
- **Split** — a draggable `GtkPaned`; the source pane cannot be shrunk to nothing.
- Light and dark themes follow the GTK/KDE colour-scheme preference.
- Open / save local `.md` files, with an unsaved-changes prompt on close.

## Requirements

Runtime libraries (Ubuntu 24.04 package names):

```
libgtk-4-1 libgtksourceview-5-0 libwebkitgtk-6.0-4
```

Build dependencies:

```
libgtk-4-dev libgtksourceview-5-dev libwebkitgtk-6.0-dev pkg-config build-essential
```

A Rust toolchain (edition 2021; developed against Rust 1.98).

## Build

```sh
cargo build --release
```

The binary is written to `target/release/mdedit`.

## Run

```sh
./target/release/mdedit [FILE.md]
```

Keyboard shortcuts:

| Action            | Shortcut            |
|-------------------|---------------------|
| New               | `Ctrl+N`            |
| Open              | `Ctrl+O`            |
| Save              | `Ctrl+S`            |
| Save As           | `Ctrl+Shift+S`      |
| Toggle preview    | `F9`                |
| Quit              | `Ctrl+Q`            |

## Headless rendering

`--render` parses a file, prints the rendered **HTML fragment** to stdout and
exits without ever touching GTK or the display. This is the primary test hook.

```sh
./target/release/mdedit --render README.md
./target/release/mdedit --version
```

Example:

```
$ printf '# Hi\n\n| a | b |\n|---|---|\n| 1 | 2 |\n' > sample.md
$ ./target/release/mdedit --render sample.md
<h1>Hi</h1>
<table>
...
</table>
```

## Tests

```sh
cargo test
```

The unit tests cover GFM rendering (tables, task lists, strikethrough,
autolinks) and, importantly, that **raw HTML is escaped and never executed**
and that `javascript:` links are neutralised.

## Safety notes

- Raw HTML in Markdown is escaped (`comrak` `render.escape = true`,
  `render.unsafe = false`), so `<script>` and friends cannot execute.
- The preview's own JavaScript is only used to swap the inner HTML of the
  content container and preserve scroll position; user content is never
  interpolated into script.
- Files are read as UTF-8; a non-UTF-8 file reports a dialog and leaves the
  current document untouched. CRLF/CR line endings are normalised to LF on
  load and written back as LF.
- Relative image paths resolve against the opened file's directory via the
  WebView base URI.
- On Ubuntu 24.04 with AppArmor's unprivileged user namespace restriction
  (`kernel.apparmor_restrict_unprivileged_userns=1`), WebKitGTK's sandbox
  cannot be set up for unconfined binaries. The binary detects this at startup
  and sets `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` to allow the WebView to
  launch. An alternative is to create an AppArmor profile that grants `userns`
  to the binary, which keeps the sandbox enabled.

## Install the launcher (per user, no sudo)

The desktop entry and icon are under `packaging/`. To install:

```sh
mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
cp packaging/mdedit.desktop ~/.local/share/applications/mdedit.desktop
cp packaging/mdedit.svg ~/.local/share/icons/hicolor/scalable/apps/mdedit.svg
update-desktop-database ~/.local/share/applications
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
```

`Exec` in the desktop entry points at
`/home/richard/mdedit/target/release/mdedit`; edit it if you move the binary.

## Project layout

```
src/main.rs       CLI parsing (--version / --render / GUI), headless render
src/markdown.rs   pure Markdown -> sanitised HTML (comrak), unit tests
src/app.rs        shared AppState (path, modified, debounce, dark mode)
src/editor.rs     GtkSourceView 5 editor pane
src/preview.rs    WebKitGTK 6.0 preview shell + JS injection
src/window.rs     GtkApplicationWindow: layout, actions, live-preview pipeline
src/file_ops.rs   file read/write + file/alert dialogs
resources/preview.css  light/dark preview stylesheet
packaging/        .desktop entry + SVG icon
```
