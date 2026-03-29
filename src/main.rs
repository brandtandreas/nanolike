mod config;
mod editor;
mod ui;

use std::io::{self, stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};

use config::{config_path, keybindings_path, Config, KeyBindings};
use editor::Editor;

// ── Key event → name string ───────────────────────────────────────────────────

fn key_event_to_name(ev: &KeyEvent) -> String {
    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
    let alt = ev.modifiers.contains(KeyModifiers::ALT);
    let shift = ev.modifiers.contains(KeyModifiers::SHIFT);

    match ev.code {
        KeyCode::Char(c) => {
            let lc = c.to_lowercase().next().unwrap_or(c);
            match (ctrl, alt, shift) {
                (true, _, true) => format!("ctrl+shift+{}", lc),
                (true, _, false) => format!("ctrl+{}", lc),
                (false, true, _) => format!("alt+{}", c),
                _ => c.to_string(),
            }
        }
        KeyCode::F(n) => format!("f{}", n),
        KeyCode::Up => match (ctrl, alt) {
            (true, _) => "ctrl+up".to_string(),
            (_, true) => "alt+up".to_string(),
            _ => "up".to_string(),
        },
        KeyCode::Down => match (ctrl, alt) {
            (true, _) => "ctrl+down".to_string(),
            (_, true) => "alt+down".to_string(),
            _ => "down".to_string(),
        },
        KeyCode::Left => match (ctrl, alt) {
            (true, _) => "ctrl+left".to_string(),
            (_, true) => "alt+left".to_string(),
            _ => "left".to_string(),
        },
        KeyCode::Right => match (ctrl, alt) {
            (true, _) => "ctrl+right".to_string(),
            (_, true) => "alt+right".to_string(),
            _ => "right".to_string(),
        },
        KeyCode::Home => {
            if ctrl { "ctrl+home".to_string() } else { "home".to_string() }
        }
        KeyCode::End => {
            if ctrl { "ctrl+end".to_string() } else { "end".to_string() }
        }
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Backspace => match (ctrl, alt) {
            (true, _) => "ctrl+backspace".to_string(),
            (_, true) => "alt+backspace".to_string(),
            _ => "backspace".to_string(),
        },
        KeyCode::Delete => {
            if ctrl { "ctrl+delete".to_string() } else { "delete".to_string() }
        }
        KeyCode::Tab => {
            if shift { "shift+tab".to_string() } else { "tab".to_string() }
        }
        KeyCode::BackTab => "shift+tab".to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "escape".to_string(),
        _ => format!("unknown:{:?}", ev.code),
    }
}

// ── Terminal cleanup guard ────────────────────────────────────────────────────

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

// ── Application state ─────────────────────────────────────────────────────────

struct App {
    editor: Editor,
    config: Config,
    kb: KeyBindings,
    stdout: io::Stdout,
    running: bool,
    term_h: usize,
    term_w: usize,
}

impl App {
    fn new(filename: Option<String>) -> io::Result<Self> {
        let (w, h) = terminal::size()?;
        Ok(Self {
            editor: Editor::new(filename),
            config: Config::load(),
            kb: KeyBindings::load(),
            stdout: stdout(),
            running: true,
            term_h: h as usize,
            term_w: w as usize,
        })
    }

    fn text_height(&self) -> usize {
        self.term_h.saturating_sub(3)
    }

    fn text_width(&self) -> usize {
        let lnw = ui::line_number_width(&self.editor, &self.config);
        self.term_w.saturating_sub(lnw)
    }

    fn render(&mut self) -> io::Result<()> {
        ui::render(&mut self.stdout, &self.editor, &self.config, &self.kb)
    }

    fn prompt(&mut self, msg: &str, default: &str) -> io::Result<Option<String>> {
        ui::prompt(&mut self.stdout, msg, default, self.term_h, self.term_w)
    }

    fn prompt_non_empty(&mut self, msg: &str, default: &str) -> io::Result<Option<String>> {
        match self.prompt(msg, default)? {
            Some(s) if !s.is_empty() => Ok(Some(s)),
            _ => Ok(None),
        }
    }

    /// Save to the existing filename, or prompt for one if none is set.
    /// Returns `true` if the file was saved successfully.
    fn save_current_or_prompt(&mut self) -> io::Result<bool> {
        if let Some(fname) = self.editor.filename.clone() {
            return Ok(self.editor.save_file(&fname));
        }
        let fname = match self.prompt_non_empty("Save as: ", "")? {
            Some(f) => f,
            None => return Ok(false),
        };
        Ok(self.editor.save_file(&fname))
    }

    fn handle_quit(&mut self) -> io::Result<()> {
        if !self.editor.modified {
            self.running = false;
            return Ok(());
        }
        let choice = match self.prompt("Save before quit? (y/n/[cancel]): ", "")? {
            Some(c) => c,
            None => return Ok(()), // Esc = cancel
        };
        match choice.trim().to_lowercase().as_str() {
            "y" | "yes" => {
                if self.save_current_or_prompt()? {
                    self.running = false;
                }
            }
            "n" | "no" => self.running = false,
            _ => {} // cancel or anything else
        }
        Ok(())
    }

    fn handle_goto_line(&mut self) -> io::Result<()> {
        let s = match self.prompt("Go to line: ", "")? {
            Some(s) => s,
            None => return Ok(()),
        };
        let n: usize = match s.trim().parse() {
            Ok(n) if n >= 1 => n,
            _ => {
                self.editor.set_status("Invalid line number", true);
                return Ok(());
            }
        };
        let n0 = n - 1;
        if n0 >= self.editor.lines.len() {
            self.editor.set_status(format!("Line {} out of range", n), true);
            return Ok(());
        }
        self.editor.goto_line(n0);
        self.editor.set_status(format!("Jumped to line {}", n), false);
        Ok(())
    }

    fn handle_replace(&mut self) -> io::Result<()> {
        let default = self.editor.search_term.clone();
        let term = match self.prompt_non_empty("Search: ", &default)? {
            Some(t) => t,
            None => {
                self.editor.set_status("Replace cancelled", false);
                return Ok(());
            }
        };
        // Replacement may be empty (delete all occurrences).
        let replacement = match self.prompt("Replace with: ", "")? {
            Some(r) => r,
            None => {
                self.editor.set_status("Replace cancelled", false);
                return Ok(());
            }
        };
        let choice = match self.prompt("Replace all? (y/n): ", "")? {
            Some(c) => c,
            None => {
                self.editor.set_status("Replace cancelled", false);
                return Ok(());
            }
        };
        self.editor.search_term = term.clone();
        if choice.trim().to_lowercase().starts_with('y') {
            self.editor.replace_all(&term, &replacement);
            self.editor.search_matches.clear();
        } else {
            self.editor.build_search_matches();
            self.editor.search_next();
        }
        Ok(())
    }

    // ── Action dispatch ───────────────────────────────────────────────────────

    fn handle_action(&mut self, action: &str) -> io::Result<()> {
        match action {
            "quit" => self.handle_quit()?,

            "save" => {
                if let Some(fname) = self.editor.filename.clone() {
                    self.editor.save_file(&fname);
                } else {
                    match self.prompt("Save as: ", "")? {
                        Some(fname) if !fname.is_empty() => {
                            self.editor.save_file(&fname);
                        }
                        _ => self.editor.set_status("Save cancelled", false),
                    }
                }
            }

            "save_as" => {
                let default = self.editor.filename.clone().unwrap_or_default();
                match self.prompt("Save as: ", &default)? {
                    Some(fname) if !fname.is_empty() => {
                        self.editor.save_file(&fname);
                    }
                    _ => self.editor.set_status("Save cancelled", false),
                }
            }

            "help" => {
                ui::show_help(&mut self.stdout, &self.kb, self.term_h, self.term_w)?;
            }

            "cut_line" => self.editor.cut_line(),
            "copy_line" => self.editor.copy_line(),
            "paste" => self.editor.paste(),

            "search" => {
                let default = self.editor.search_term.clone();
                match self.prompt("Search: ", &default)? {
                    Some(term) if !term.is_empty() => {
                        self.editor.search_term = term;
                        self.editor.build_search_matches();
                        if self.editor.search_matches.is_empty() {
                            let msg = format!("Not found: {}", self.editor.search_term);
                            self.editor.set_status(msg, true);
                        } else {
                            self.editor.search_next();
                        }
                    }
                    Some(_) => {
                        self.editor.search_term.clear();
                        self.editor.search_matches.clear();
                        self.editor.set_status("Search cleared", false);
                    }
                    None => self.editor.set_status("Search cancelled", false),
                }
            }

            "search_next" => {
                if self.editor.search_term.is_empty() {
                    self.handle_action("search")?;
                } else {
                    self.editor.build_search_matches();
                    if !self.editor.search_next() {
                        let msg = format!("Not found: {}", self.editor.search_term);
                        self.editor.set_status(msg, true);
                    }
                }
            }

            "search_prev" => {
                if self.editor.search_term.is_empty() {
                    self.handle_action("search")?;
                } else {
                    self.editor.build_search_matches();
                    if !self.editor.search_prev() {
                        let msg = format!("Not found: {}", self.editor.search_term);
                        self.editor.set_status(msg, true);
                    }
                }
            }

            "replace" => self.handle_replace()?,

            "goto_line" => self.handle_goto_line()?,

            "page_up" => self.editor.move_page_up(self.text_height()),
            "page_down" => self.editor.move_page_down(self.text_height()),

            "file_top" => {
                self.editor.cursor_row = 0;
                self.editor.cursor_col = 0;
            }
            "file_bottom" => {
                self.editor.cursor_row = self.editor.lines.len().saturating_sub(1);
                self.editor.cursor_col =
                    self.editor.lines.last().map(|l| l.len()).unwrap_or(0);
            }

            "undo" => self.editor.do_undo(),
            "redo" => self.editor.do_redo(),

            "next_word" => self.editor.move_next_word(),
            "prev_word" => self.editor.move_prev_word(),
            "delete_word" => self.editor.delete_word_before(),
            "delete_to_eol" => self.editor.delete_to_eol(),

            "toggle_line_numbers" => {
                self.config.line_numbers = !self.config.line_numbers;
                self.config.save();
                let on = self.config.line_numbers;
                self.editor.set_status(
                    format!("Line numbers {}", if on { "on" } else { "off" }),
                    false,
                );
            }
            "toggle_word_wrap" => {
                self.config.word_wrap = !self.config.word_wrap;
                self.config.save();
                let on = self.config.word_wrap;
                self.editor
                    .set_status(format!("Word wrap {}", if on { "on" } else { "off" }), false);
            }
            "toggle_auto_indent" => {
                self.config.auto_indent = !self.config.auto_indent;
                self.config.save();
                let on = self.config.auto_indent;
                self.editor.set_status(
                    format!("Auto-indent {}", if on { "on" } else { "off" }),
                    false,
                );
            }

            _ => {}
        }
        Ok(())
    }

    // ── Key event handler ─────────────────────────────────────────────────────

    fn handle_key(&mut self, ev: KeyEvent) -> io::Result<()> {
        // Ignore key-release events (some terminals emit them).
        if ev.kind == KeyEventKind::Release {
            return Ok(());
        }

        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        let alt = ev.modifiers.contains(KeyModifiers::ALT);

        // ── Unmodified navigation (highest priority, always available) ─────────
        match ev.code {
            KeyCode::Up => {
                self.editor.move_up();
                return Ok(());
            }
            KeyCode::Down => {
                self.editor.move_down();
                return Ok(());
            }
            KeyCode::Left if !ctrl && !alt => {
                self.editor.move_left();
                return Ok(());
            }
            KeyCode::Right if !ctrl && !alt => {
                self.editor.move_right();
                return Ok(());
            }
            KeyCode::Home if !ctrl => {
                self.editor.move_home();
                return Ok(());
            }
            KeyCode::End if !ctrl => {
                self.editor.move_end();
                return Ok(());
            }
            KeyCode::PageUp if !ctrl && !alt => {
                self.editor.move_page_up(self.text_height());
                return Ok(());
            }
            KeyCode::PageDown if !ctrl && !alt => {
                self.editor.move_page_down(self.text_height());
                return Ok(());
            }
            KeyCode::Enter => {
                self.editor.insert_newline(self.config.auto_indent);
                return Ok(());
            }
            KeyCode::Backspace if !ctrl && !alt => {
                self.editor.backspace();
                return Ok(());
            }
            KeyCode::Delete if !ctrl && !alt => {
                self.editor.delete_char();
                return Ok(());
            }
            KeyCode::Tab => {
                if self.config.use_spaces {
                    self.editor.insert_str(&" ".repeat(self.config.tab_size));
                } else {
                    self.editor.insert_char('\t');
                }
                return Ok(());
            }
            _ => {}
        }

        // ── Keybinding lookup ─────────────────────────────────────────────────
        let key_name = key_event_to_name(&ev);
        if let Some(action) = self.kb.get_action(&key_name).map(|s| s.to_string()) {
            return self.handle_action(&action);
        }

        // ── Printable character fallthrough ───────────────────────────────────
        if let KeyCode::Char(c) = ev.code {
            if !ctrl && !alt {
                self.editor.insert_char(c);
            }
        }

        Ok(())
    }

    // ── Main loop ─────────────────────────────────────────────────────────────

    fn run(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        execute!(self.stdout, EnterAlternateScreen)?;
        let _guard = TerminalGuard;

        while self.running {
            let (w, h) = terminal::size()?;
            self.term_w = w as usize;
            self.term_h = h as usize;

            self.editor
                .update_scroll(self.text_height(), self.text_width());
            self.render()?;

            match event::read()? {
                Event::Key(ev) => self.handle_key(ev)?,
                Event::Resize(nw, nh) => {
                    self.term_w = nw as usize;
                    self.term_h = nh as usize;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("NanoLike - A nano-inspired terminal editor (Rust)");
        println!();
        println!("Usage:  nanolike [OPTIONS] [FILE]");
        println!();
        println!("Options:");
        println!("  --export-keybindings  Write default keybindings to config dir and exit");
        println!("  --export-config       Write default config to config dir and exit");
        println!("  --help, -h            Show this help message");
        println!();
        println!("Config:      {}", config_path().display());
        println!("Keybindings: {}", keybindings_path().display());
        return;
    }

    if args.iter().any(|a| a == "--export-keybindings") {
        let kb = KeyBindings::load();
        kb.save_defaults();
        println!(
            "Default keybindings exported to: {}",
            keybindings_path().display()
        );
        return;
    }

    if args.iter().any(|a| a == "--export-config") {
        let cfg = Config::default();
        cfg.save();
        println!("Default config exported to: {}", config_path().display());
        return;
    }

    let filename = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned();

    let mut app = match App::new(filename) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = app.run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
