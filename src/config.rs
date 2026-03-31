use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("nanolike")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn keybindings_path() -> PathBuf {
    config_dir().join("keybindings.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub tab_size: usize,
    pub use_spaces: bool,
    pub auto_indent: bool,
    pub word_wrap: bool,
    pub line_numbers: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tab_size: 4,
            use_spaces: true,
            auto_indent: true,
            word_wrap: false,
            line_numbers: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if !path.exists() {
            return Self::default();
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, data);
        }
    }
}

pub fn default_keybindings() -> HashMap<String, Vec<String>> {
    [
        ("quit",                 &["ctrl+x", "ctrl+q"] as &[&str]),
        ("save",                 &["ctrl+s", "ctrl+o"]),
        ("save_as",              &["ctrl+shift+s"]),
        ("help",                 &["ctrl+g", "f1"]),
        ("cut_line",             &["ctrl+k"]),
        ("paste",                &["ctrl+u", "ctrl+v"]),
        ("copy_line",            &["alt+c", "ctrl+c"]),
        ("select_all",           &["ctrl+a"]),
        ("search",               &["ctrl+f"]),
        ("search_next",          &["f3"]),
        ("search_prev",          &["ctrl+p"]),
        ("replace",              &["ctrl+r", "ctrl+h"]),
        ("goto_line",            &["ctrl+l"]),
        ("page_up",              &["pageup"]),
        ("page_down",            &["pagedown"]),
        ("file_top",             &["ctrl+home"]),
        ("file_bottom",          &["ctrl+end"]),
        ("undo",                 &["ctrl+z"]),
        ("redo",                 &["ctrl+y", "ctrl+shift+z"]),
        ("next_word",            &["alt+right", "ctrl+right"]),
        ("prev_word",            &["alt+left", "ctrl+left"]),
        ("delete_word",          &["ctrl+backspace", "alt+backspace"]),
        ("delete_to_eol",        &["alt+d"]),
        ("toggle_line_numbers",  &["alt+n"]),
        ("toggle_word_wrap",     &["alt+w"]),
        ("toggle_auto_indent",   &["alt+i"]),
        ("open_file",            &["alt+o"]),
        ("next_tab",             &["alt+."]),
        ("prev_tab",             &["alt+,"]),
        ("close_tab",            &["ctrl+w"]),
        ("new_tab",              &["ctrl+t"]),
    ]
    .iter()
    .map(|(k, v)| {
        (
            k.to_string(),
            v.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
    })
    .collect()
}

pub struct KeyBindings {
    pub bindings: HashMap<String, Vec<String>>,
    reverse: HashMap<String, String>,
}

impl KeyBindings {
    pub fn load() -> Self {
        let bindings = Self::load_bindings();
        let mut kb = Self {
            bindings,
            reverse: HashMap::new(),
        };
        kb.build_reverse();
        kb
    }

    fn load_bindings() -> HashMap<String, Vec<String>> {
        let mut merged = default_keybindings();
        let path = keybindings_path();
        if !path.exists() {
            return merged;
        }
        if let Some(user) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<HashMap<String, Vec<String>>>(&data).ok())
        {
            merged.extend(user);
        }
        merged
    }

    fn build_reverse(&mut self) {
        self.reverse.clear();
        for (action, keys) in &self.bindings {
            for key in keys {
                self.reverse.insert(key.clone(), action.clone());
            }
        }
    }

    pub fn get_action(&self, key_name: &str) -> Option<&str> {
        self.reverse.get(key_name).map(|s| s.as_str())
    }

    pub fn first_key(&self, action: &str) -> &str {
        self.bindings
            .get(action)
            .and_then(|v| v.first())
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn save_defaults(&self) {
        let path = keybindings_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(&default_keybindings()) {
            let _ = std::fs::write(path, data);
        }
    }

    #[allow(dead_code)]
    pub fn save(&self) {
        let path = keybindings_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(&self.bindings) {
            let _ = std::fs::write(path, data);
        }
    }
}
