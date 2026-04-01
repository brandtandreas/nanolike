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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: build KeyBindings directly from default keybindings (no filesystem).
    fn make_keybindings() -> KeyBindings {
        let bindings = default_keybindings();
        let mut kb = KeyBindings {
            bindings,
            reverse: HashMap::new(),
        };
        kb.build_reverse();
        kb
    }

    // ── Config defaults ───────────────────────────────────────────────────────

    #[test]
    fn config_default_values() {
        let cfg = Config::default();
        assert_eq!(cfg.tab_size, 4);
        assert!(cfg.use_spaces);
        assert!(cfg.auto_indent);
        assert!(!cfg.word_wrap);
        assert!(cfg.line_numbers);
    }

    #[test]
    fn config_serde_round_trip() {
        let cfg = Config {
            tab_size: 2,
            use_spaces: false,
            auto_indent: false,
            word_wrap: true,
            line_numbers: false,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.tab_size, 2);
        assert!(!restored.use_spaces);
        assert!(!restored.auto_indent);
        assert!(restored.word_wrap);
        assert!(!restored.line_numbers);
    }

    #[test]
    fn config_partial_json_uses_defaults() {
        // Only override tab_size; everything else should come from Default.
        let partial = r#"{"tab_size": 8}"#;
        let cfg: Config = serde_json::from_str(partial).unwrap();
        assert_eq!(cfg.tab_size, 8);
        assert!(cfg.use_spaces);    // default
        assert!(cfg.auto_indent);   // default
        assert!(!cfg.word_wrap);    // default
        assert!(cfg.line_numbers);  // default
    }

    // ── default_keybindings ───────────────────────────────────────────────────

    #[test]
    fn default_keybindings_has_expected_actions() {
        let kb = default_keybindings();
        let expected_actions = [
            "quit", "save", "save_as", "help", "cut_line", "paste",
            "copy_line", "select_all", "search", "search_next", "search_prev",
            "replace", "goto_line", "page_up", "page_down", "file_top",
            "file_bottom", "undo", "redo", "next_word", "prev_word",
            "delete_word", "delete_to_eol", "toggle_line_numbers",
            "toggle_word_wrap", "toggle_auto_indent", "open_file",
            "next_tab", "prev_tab", "close_tab", "new_tab",
        ];
        for action in &expected_actions {
            assert!(kb.contains_key(*action), "Missing action: {}", action);
        }
    }

    #[test]
    fn default_keybindings_quit_has_keys() {
        let kb = default_keybindings();
        let keys = &kb["quit"];
        assert!(keys.contains(&"ctrl+x".to_string()));
        assert!(keys.contains(&"ctrl+q".to_string()));
    }

    // ── KeyBindings ───────────────────────────────────────────────────────────

    #[test]
    fn keybindings_get_action_known_key() {
        let kb = make_keybindings();
        assert_eq!(kb.get_action("ctrl+x"), Some("quit"));
        assert_eq!(kb.get_action("ctrl+z"), Some("undo"));
        assert_eq!(kb.get_action("ctrl+s"), Some("save"));
    }

    #[test]
    fn keybindings_get_action_unknown_key() {
        let kb = make_keybindings();
        assert!(kb.get_action("ctrl+shift+f99").is_none());
    }

    #[test]
    fn keybindings_first_key() {
        let kb = make_keybindings();
        assert_eq!(kb.first_key("quit"), "ctrl+x");
        assert_eq!(kb.first_key("undo"), "ctrl+z");
    }

    #[test]
    fn keybindings_first_key_unknown_action() {
        let kb = make_keybindings();
        assert_eq!(kb.first_key("nonexistent_action"), "");
    }

    #[test]
    fn keybindings_all_default_keys_resolve_to_actions() {
        let kb = make_keybindings();
        for (action, keys) in default_keybindings() {
            for key in keys {
                let resolved = kb.get_action(&key);
                assert!(
                    resolved.is_some(),
                    "Key '{}' for action '{}' not resolvable",
                    key,
                    action
                );
            }
        }
    }

    #[test]
    fn keybindings_custom_override_replaces_default() {
        let mut bindings = default_keybindings();
        // Override "quit" with a custom key
        bindings.insert("quit".to_string(), vec!["ctrl+shift+q".to_string()]);
        let mut kb = KeyBindings {
            bindings,
            reverse: HashMap::new(),
        };
        kb.build_reverse();
        // Old key should no longer map to quit
        assert_ne!(kb.get_action("ctrl+x"), Some("quit"));
        // New key should map to quit
        assert_eq!(kb.get_action("ctrl+shift+q"), Some("quit"));
    }
}
