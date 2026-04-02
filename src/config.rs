use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// All named editor actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Quit, Save, SaveAs, Help,
    CutLine, CopyLine, Paste, SelectAll,
    Search, SearchNext, SearchPrev, Replace,
    GotoLine, PageUp, PageDown, FileTop, FileBottom,
    Undo, Redo,
    NextWord, PrevWord, DeleteWord, DeleteToEol,
    ToggleLineNumbers, ToggleWordWrap, ToggleAutoIndent,
    OpenFile, NextTab, PrevTab, CloseTab, NewTab,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Action::Quit               => "quit",
            Action::Save               => "save",
            Action::SaveAs             => "save_as",
            Action::Help               => "help",
            Action::CutLine            => "cut_line",
            Action::CopyLine           => "copy_line",
            Action::Paste              => "paste",
            Action::SelectAll          => "select_all",
            Action::Search             => "search",
            Action::SearchNext         => "search_next",
            Action::SearchPrev         => "search_prev",
            Action::Replace            => "replace",
            Action::GotoLine           => "goto_line",
            Action::PageUp             => "page_up",
            Action::PageDown           => "page_down",
            Action::FileTop            => "file_top",
            Action::FileBottom         => "file_bottom",
            Action::Undo               => "undo",
            Action::Redo               => "redo",
            Action::NextWord           => "next_word",
            Action::PrevWord           => "prev_word",
            Action::DeleteWord         => "delete_word",
            Action::DeleteToEol        => "delete_to_eol",
            Action::ToggleLineNumbers  => "toggle_line_numbers",
            Action::ToggleWordWrap     => "toggle_word_wrap",
            Action::ToggleAutoIndent   => "toggle_auto_indent",
            Action::OpenFile           => "open_file",
            Action::NextTab            => "next_tab",
            Action::PrevTab            => "prev_tab",
            Action::CloseTab           => "close_tab",
            Action::NewTab             => "new_tab",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for Action {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "quit"                => Ok(Action::Quit),
            "save"                => Ok(Action::Save),
            "save_as"             => Ok(Action::SaveAs),
            "help"                => Ok(Action::Help),
            "cut_line"            => Ok(Action::CutLine),
            "copy_line"           => Ok(Action::CopyLine),
            "paste"               => Ok(Action::Paste),
            "select_all"          => Ok(Action::SelectAll),
            "search"              => Ok(Action::Search),
            "search_next"         => Ok(Action::SearchNext),
            "search_prev"         => Ok(Action::SearchPrev),
            "replace"             => Ok(Action::Replace),
            "goto_line"           => Ok(Action::GotoLine),
            "page_up"             => Ok(Action::PageUp),
            "page_down"           => Ok(Action::PageDown),
            "file_top"            => Ok(Action::FileTop),
            "file_bottom"         => Ok(Action::FileBottom),
            "undo"                => Ok(Action::Undo),
            "redo"                => Ok(Action::Redo),
            "next_word"           => Ok(Action::NextWord),
            "prev_word"           => Ok(Action::PrevWord),
            "delete_word"         => Ok(Action::DeleteWord),
            "delete_to_eol"       => Ok(Action::DeleteToEol),
            "toggle_line_numbers" => Ok(Action::ToggleLineNumbers),
            "toggle_word_wrap"    => Ok(Action::ToggleWordWrap),
            "toggle_auto_indent"  => Ok(Action::ToggleAutoIndent),
            "open_file"           => Ok(Action::OpenFile),
            "next_tab"            => Ok(Action::NextTab),
            "prev_tab"            => Ok(Action::PrevTab),
            "close_tab"           => Ok(Action::CloseTab),
            "new_tab"             => Ok(Action::NewTab),
            _                     => Err(()),
        }
    }
}

impl serde::Serialize for Action {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

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

pub fn default_keybindings() -> HashMap<Action, Vec<String>> {
    [
        (Action::Quit,              &["ctrl+x", "ctrl+q"] as &[&str]),
        (Action::Save,              &["ctrl+s", "ctrl+o"]),
        (Action::SaveAs,            &["ctrl+shift+s"]),
        (Action::Help,              &["ctrl+g", "f1"]),
        (Action::CutLine,           &["ctrl+k"]),
        (Action::Paste,             &["ctrl+u", "ctrl+v"]),
        (Action::CopyLine,          &["alt+c", "ctrl+c"]),
        (Action::SelectAll,         &["ctrl+a"]),
        (Action::Search,            &["ctrl+f"]),
        (Action::SearchNext,        &["f3"]),
        (Action::SearchPrev,        &["ctrl+p"]),
        (Action::Replace,           &["ctrl+r", "ctrl+h"]),
        (Action::GotoLine,          &["ctrl+l"]),
        (Action::PageUp,            &["pageup"]),
        (Action::PageDown,          &["pagedown"]),
        (Action::FileTop,           &["ctrl+home"]),
        (Action::FileBottom,        &["ctrl+end"]),
        (Action::Undo,              &["ctrl+z"]),
        (Action::Redo,              &["ctrl+y", "ctrl+shift+z"]),
        (Action::NextWord,          &["alt+right", "ctrl+right"]),
        (Action::PrevWord,          &["alt+left", "ctrl+left"]),
        (Action::DeleteWord,        &["ctrl+backspace", "alt+backspace"]),
        (Action::DeleteToEol,       &["alt+d"]),
        (Action::ToggleLineNumbers, &["alt+n"]),
        (Action::ToggleWordWrap,    &["alt+w"]),
        (Action::ToggleAutoIndent,  &["alt+i"]),
        (Action::OpenFile,          &["alt+o"]),
        (Action::NextTab,           &["alt+."]),
        (Action::PrevTab,           &["alt+,"]),
        (Action::CloseTab,          &["ctrl+w"]),
        (Action::NewTab,            &["ctrl+t"]),
    ]
    .iter()
    .map(|(a, v)| (*a, v.iter().map(|s| s.to_string()).collect::<Vec<_>>()))
    .collect()
}

pub struct KeyBindings {
    pub bindings: HashMap<Action, Vec<String>>,
    reverse: HashMap<String, Action>,
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

    fn load_bindings() -> HashMap<Action, Vec<String>> {
        let mut merged = default_keybindings();
        let path = keybindings_path();
        if !path.exists() {
            return merged;
        }
        if let Some(user) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<HashMap<String, Vec<String>>>(&data).ok())
        {
            for (key_str, binds) in user {
                if let Ok(action) = key_str.parse::<Action>() {
                    merged.insert(action, binds);
                }
                // Unknown action names are silently skipped for backward compatibility.
            }
        }
        merged
    }

    fn build_reverse(&mut self) {
        self.reverse.clear();
        for (action, keys) in &self.bindings {
            for key in keys {
                self.reverse.insert(key.clone(), *action);
            }
        }
    }

    pub fn get_action(&self, key_name: &str) -> Option<Action> {
        self.reverse.get(key_name).copied()
    }

    pub fn first_key(&self, action: Action) -> &str {
        self.bindings
            .get(&action)
            .and_then(|v| v.first())
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn save_defaults(&self) {
        let path = keybindings_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Serialize as HashMap<String, Vec<String>> to produce the standard JSON format.
        let string_map: HashMap<String, Vec<String>> = default_keybindings()
            .into_iter()
            .map(|(a, v)| (a.to_string(), v))
            .collect();
        if let Ok(data) = serde_json::to_string_pretty(&string_map) {
            let _ = std::fs::write(path, data);
        }
    }

    #[allow(dead_code)]
    pub fn save(&self) {
        let path = keybindings_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let string_map: HashMap<String, Vec<String>> = self.bindings
            .iter()
            .map(|(a, v)| (a.to_string(), v.clone()))
            .collect();
        if let Ok(data) = serde_json::to_string_pretty(&string_map) {
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
            Action::Quit, Action::Save, Action::SaveAs, Action::Help,
            Action::CutLine, Action::Paste, Action::CopyLine, Action::SelectAll,
            Action::Search, Action::SearchNext, Action::SearchPrev, Action::Replace,
            Action::GotoLine, Action::PageUp, Action::PageDown, Action::FileTop,
            Action::FileBottom, Action::Undo, Action::Redo, Action::NextWord,
            Action::PrevWord, Action::DeleteWord, Action::DeleteToEol,
            Action::ToggleLineNumbers, Action::ToggleWordWrap, Action::ToggleAutoIndent,
            Action::OpenFile, Action::NextTab, Action::PrevTab, Action::CloseTab,
            Action::NewTab,
        ];
        for action in &expected_actions {
            assert!(kb.contains_key(action), "Missing action: {}", action);
        }
    }

    #[test]
    fn default_keybindings_quit_has_keys() {
        let kb = default_keybindings();
        let keys = &kb[&Action::Quit];
        assert!(keys.contains(&"ctrl+x".to_string()));
        assert!(keys.contains(&"ctrl+q".to_string()));
    }

    // ── KeyBindings ───────────────────────────────────────────────────────────

    #[test]
    fn keybindings_get_action_known_key() {
        let kb = make_keybindings();
        assert_eq!(kb.get_action("ctrl+x"), Some(Action::Quit));
        assert_eq!(kb.get_action("ctrl+z"), Some(Action::Undo));
        assert_eq!(kb.get_action("ctrl+s"), Some(Action::Save));
    }

    #[test]
    fn keybindings_get_action_unknown_key() {
        let kb = make_keybindings();
        assert!(kb.get_action("ctrl+shift+f99").is_none());
    }

    #[test]
    fn keybindings_first_key() {
        let kb = make_keybindings();
        assert_eq!(kb.first_key(Action::Quit), "ctrl+x");
        assert_eq!(kb.first_key(Action::Undo), "ctrl+z");
    }

    #[test]
    fn keybindings_first_key_unknown_action_returns_empty() {
        // Actions with no bindings return "". We can test this by removing a binding.
        let mut bindings = default_keybindings();
        bindings.remove(&Action::Quit);
        let mut kb = KeyBindings { bindings, reverse: HashMap::new() };
        kb.build_reverse();
        assert_eq!(kb.first_key(Action::Quit), "");
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
        // Override Quit with a custom key
        bindings.insert(Action::Quit, vec!["ctrl+shift+q".to_string()]);
        let mut kb = KeyBindings { bindings, reverse: HashMap::new() };
        kb.build_reverse();
        // Old keys should no longer map to Quit
        assert_ne!(kb.get_action("ctrl+x"), Some(Action::Quit));
        assert_ne!(kb.get_action("ctrl+q"), Some(Action::Quit));
        // New key should map to Quit
        assert_eq!(kb.get_action("ctrl+shift+q"), Some(Action::Quit));
    }

    // ── Action Display / FromStr ──────────────────────────────────────────────

    #[test]
    fn action_display_fromstr_roundtrip() {
        let pairs = [
            (Action::Quit,              "quit"),
            (Action::SearchNext,        "search_next"),
            (Action::ToggleLineNumbers, "toggle_line_numbers"),
            (Action::NewTab,            "new_tab"),
        ];
        for (action, expected_str) in pairs {
            assert_eq!(action.to_string(), expected_str, "Display mismatch for {:?}", action);
            assert_eq!(expected_str.parse::<Action>(), Ok(action), "FromStr mismatch for {}", expected_str);
        }
    }

    #[test]
    fn action_fromstr_unknown_returns_err() {
        assert!("not_an_action".parse::<Action>().is_err());
        assert!("Quit".parse::<Action>().is_err()); // case-sensitive
        assert!("".parse::<Action>().is_err());
    }
}
