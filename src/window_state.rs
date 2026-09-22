//! Window size, maximised state and split position, remembered between runs
//! in `$XDG_CONFIG_HOME/mdedit/window.ini`.

use std::path::PathBuf;

use gtk4::glib;

const GROUP: &str = "window";

#[derive(Clone, Copy, Debug)]
pub struct WindowState {
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
    /// Position of the editor/preview divider, in pixels.
    pub split: i32,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1100,
            height: 720,
            maximized: false,
            split: 550,
        }
    }
}

fn config_path() -> PathBuf {
    glib::user_config_dir().join("mdedit").join("window.ini")
}

impl WindowState {
    /// Load the saved state, falling back to defaults for anything missing
    /// or unreadable.
    pub fn load() -> Self {
        let defaults = Self::default();
        let file = glib::KeyFile::new();
        if file
            .load_from_file(config_path(), glib::KeyFileFlags::NONE)
            .is_err()
        {
            return defaults;
        }
        let int = |key: &str, fallback: i32| {
            file.integer(GROUP, key)
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(fallback)
        };
        Self {
            width: int("width", defaults.width),
            height: int("height", defaults.height),
            maximized: file.boolean(GROUP, "maximized").unwrap_or(false),
            split: int("split", defaults.split),
        }
    }

    /// Best-effort save; failing to persist window geometry is not worth
    /// bothering the user about.
    pub fn save(&self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let file = glib::KeyFile::new();
        file.set_integer(GROUP, "width", self.width);
        file.set_integer(GROUP, "height", self.height);
        file.set_boolean(GROUP, "maximized", self.maximized);
        file.set_integer(GROUP, "split", self.split);
        let _ = file.save_to_file(path);
    }
}
