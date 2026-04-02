mod config;
mod editor;
mod ui;

use std::io::{self, stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};

use config::{config_path, keybindings_path, Action, Config, KeyBindings};
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
    editors: Vec<Editor>,
    active_tab: usize,
    config: Config,
    kb: KeyBindings,
    stdout: io::Stdout,
    running: bool,
    term_h: usize,
    term_w: usize,
}

impl App {
    fn new(filenames: Vec<String>) -> io::Result<Self> {
        let (w, h) = terminal::size()?;
        let editors = if filenames.is_empty() {
            vec![Editor::new(None)]
        } else {
            filenames.into_iter().map(|f| Editor::new(Some(f))).collect()
        };
        Ok(Self {
            editors,
            active_tab: 0,
            config: Config::load(),
            kb: KeyBindings::load(),
            stdout: stdout(),
            running: true,
            term_h: h as usize,
            term_w: w as usize,
        })
    }

    fn text_height(&self) -> usize {
        self.term_h.saturating_sub(4)
    }

    fn text_width(&self) -> usize {
        let lnw = ui::line_number_width(&self.editors[self.active_tab], &self.config);
        self.term_w.saturating_sub(lnw)
    }

    fn render(&mut self) -> io::Result<()> {
        ui::render(&mut self.stdout, &self.editors, self.active_tab, &self.config, &self.kb)
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

    /// Save the active tab to its filename, or prompt for one if not set.
    /// Returns `true` if the file was saved successfully.
    fn save_current_or_prompt(&mut self) -> io::Result<bool> {
        let fname = self.editors[self.active_tab].filename.clone();
        if let Some(fname) = fname {
            return Ok(self.editors[self.active_tab].save_file(&fname));
        }
        let fname = match self.prompt_non_empty("Save as: ", "")? {
            Some(f) => f,
            None => return Ok(false),
        };
        Ok(self.editors[self.active_tab].save_file(&fname))
    }

    fn handle_quit(&mut self) -> io::Result<()> {
        let modified_indices: Vec<usize> = (0..self.editors.len())
            .filter(|&i| self.editors[i].modified)
            .collect();
        if modified_indices.is_empty() {
            self.running = false;
            return Ok(());
        }
        let msg = if modified_indices.len() == 1 {
            "Save before quit? (y/n/[cancel]): ".to_string()
        } else {
            format!(
                "{} tabs have unsaved changes. Save all? (y/n/[cancel]): ",
                modified_indices.len()
            )
        };
        let choice = match self.prompt(&msg, "")? {
            Some(c) => c,
            None => return Ok(()), // Esc = cancel
        };
        match choice.trim().to_lowercase().as_str() {
            "y" | "yes" => {
                for i in modified_indices {
                    self.active_tab = i;
                    if !self.save_current_or_prompt()? {
                        return Ok(()); // save cancelled or failed — abort quit
                    }
                }
                self.running = false;
            }
            "n" | "no" => self.running = false,
            _ => {}
        }
        Ok(())
    }

    fn handle_open_file(&mut self) -> io::Result<()> {
        let fname = match ui::file_prompt(
            &mut self.stdout,
            "Open file: ",
            "",
            self.term_h,
            self.term_w,
        )? {
            Some(f) if !f.is_empty() => f,
            _ => return Ok(()),
        };
        // If the current tab is a pristine empty buffer, reuse it.
        let current_is_empty = {
            let ed = &self.editors[self.active_tab];
            ed.filename.is_none() && !ed.modified && ed.lines == vec![String::new()]
        };
        if current_is_empty {
            self.editors[self.active_tab] = Editor::new(Some(fname));
        } else {
            self.editors.push(Editor::new(Some(fname)));
            self.active_tab = self.editors.len() - 1;
        }
        Ok(())
    }

    fn handle_close_tab(&mut self) -> io::Result<()> {
        if self.editors[self.active_tab].modified {
            let choice = match self.prompt("Save before closing tab? (y/n/[cancel]): ", "")? {
                Some(c) => c,
                None => return Ok(()), // Esc = cancel
            };
            match choice.trim().to_lowercase().as_str() {
                "y" | "yes" => {
                    if !self.save_current_or_prompt()? {
                        return Ok(());
                    }
                }
                "n" | "no" => {}
                _ => return Ok(()),
            }
        }
        self.editors.remove(self.active_tab);
        if self.editors.is_empty() {
            self.running = false;
        } else {
            self.active_tab = self.active_tab.min(self.editors.len() - 1);
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
                self.editors[self.active_tab].set_status("Invalid line number", true);
                return Ok(());
            }
        };
        let n0 = n - 1;
        if n0 >= self.editors[self.active_tab].lines.len() {
            self.editors[self.active_tab].set_status(format!("Line {} out of range", n), true);
            return Ok(());
        }
        self.editors[self.active_tab].goto_line(n0);
        self.editors[self.active_tab].set_status(format!("Jumped to line {}", n), false);
        Ok(())
    }

    fn handle_replace(&mut self) -> io::Result<()> {
        let default = self.editors[self.active_tab].search_term.clone();
        let term = match self.prompt_non_empty("Search: ", &default)? {
            Some(t) => t,
            None => {
                self.editors[self.active_tab].set_status("Replace cancelled", false);
                return Ok(());
            }
        };
        // Replacement may be empty (delete all occurrences).
        let replacement = match self.prompt("Replace with: ", "")? {
            Some(r) => r,
            None => {
                self.editors[self.active_tab].set_status("Replace cancelled", false);
                return Ok(());
            }
        };
        let choice = match self.prompt("Replace all? (y/n): ", "")? {
            Some(c) => c,
            None => {
                self.editors[self.active_tab].set_status("Replace cancelled", false);
                return Ok(());
            }
        };
        self.editors[self.active_tab].search_term = term.clone();
        if choice.trim().to_lowercase().starts_with('y') {
            self.editors[self.active_tab].replace_all(&term, &replacement);
            self.editors[self.active_tab].search_matches.clear();
        } else {
            self.editors[self.active_tab].build_search_matches();
            self.editors[self.active_tab].search_next();
        }
        Ok(())
    }

    // ── Action dispatch ───────────────────────────────────────────────────────

    fn handle_action(&mut self, action: Action) -> io::Result<()> {
        let t = self.active_tab;
        match action {
            Action::Quit => self.handle_quit()?,

            Action::Save => {
                let fname = self.editors[t].filename.clone();
                if let Some(fname) = fname {
                    self.editors[t].save_file(&fname);
                } else {
                    match self.prompt("Save as: ", "")? {
                        Some(fname) if !fname.is_empty() => {
                            self.editors[self.active_tab].save_file(&fname);
                        }
                        _ => self.editors[self.active_tab].set_status("Save cancelled", false),
                    }
                }
            }

            Action::SaveAs => {
                let default = self.editors[t].filename.clone().unwrap_or_default();
                match self.prompt("Save as: ", &default)? {
                    Some(fname) if !fname.is_empty() => {
                        self.editors[self.active_tab].save_file(&fname);
                    }
                    _ => self.editors[self.active_tab].set_status("Save cancelled", false),
                }
            }

            Action::Help => {
                ui::show_help(&mut self.stdout, &self.kb, self.term_h, self.term_w)?;
            }

            Action::CutLine  => self.editors[t].cut_line(),
            Action::CopyLine => self.editors[t].copy_line(),
            Action::Paste    => self.editors[t].paste(),

            Action::Search => {
                let default = self.editors[t].search_term.clone();
                match self.prompt("Search: ", &default)? {
                    Some(term) if !term.is_empty() => {
                        self.editors[self.active_tab].search_term = term;
                        self.editors[self.active_tab].build_search_matches();
                        if self.editors[self.active_tab].search_matches.is_empty() {
                            let msg =
                                format!("Not found: {}", self.editors[self.active_tab].search_term);
                            self.editors[self.active_tab].set_status(msg, true);
                        } else {
                            self.editors[self.active_tab].search_next();
                        }
                    }
                    Some(_) => {
                        self.editors[self.active_tab].search_term.clear();
                        self.editors[self.active_tab].search_matches.clear();
                        self.editors[self.active_tab].set_status("Search cleared", false);
                    }
                    None => self.editors[self.active_tab].set_status("Search cancelled", false),
                }
            }

            Action::SearchNext => {
                if self.editors[t].search_term.is_empty() {
                    self.handle_action(Action::Search)?;
                } else {
                    self.editors[self.active_tab].build_search_matches();
                    if !self.editors[self.active_tab].search_next() {
                        let msg =
                            format!("Not found: {}", self.editors[self.active_tab].search_term);
                        self.editors[self.active_tab].set_status(msg, true);
                    }
                }
            }

            Action::SearchPrev => {
                if self.editors[t].search_term.is_empty() {
                    self.handle_action(Action::Search)?;
                } else {
                    self.editors[self.active_tab].build_search_matches();
                    if !self.editors[self.active_tab].search_prev() {
                        let msg =
                            format!("Not found: {}", self.editors[self.active_tab].search_term);
                        self.editors[self.active_tab].set_status(msg, true);
                    }
                }
            }

            Action::Replace => self.handle_replace()?,

            Action::GotoLine => self.handle_goto_line()?,

            Action::PageUp   => { let h = self.text_height(); self.editors[t].move_page_up(h); }
            Action::PageDown => { let h = self.text_height(); self.editors[t].move_page_down(h); }

            Action::FileTop => {
                self.editors[t].selection_anchor = None;
                self.editors[t].cursor_row = 0;
                self.editors[t].cursor_col = 0;
            }
            Action::FileBottom => {
                self.editors[t].selection_anchor = None;
                self.editors[t].cursor_row = self.editors[t].lines.len().saturating_sub(1);
                self.editors[t].cursor_col =
                    self.editors[t].lines.last().map(|l| l.len()).unwrap_or(0);
            }

            Action::SelectAll => self.editors[t].select_all(),

            Action::Undo => self.editors[t].do_undo(),
            Action::Redo => self.editors[t].do_redo(),

            Action::NextWord   => self.editors[t].move_next_word(),
            Action::PrevWord   => self.editors[t].move_prev_word(),
            Action::DeleteWord => self.editors[t].delete_word_before(),
            Action::DeleteToEol=> self.editors[t].delete_to_eol(),

            Action::ToggleLineNumbers => {
                self.config.line_numbers = !self.config.line_numbers;
                self.config.save();
                let on = self.config.line_numbers;
                self.editors[t].set_status(
                    format!("Line numbers {}", if on { "on" } else { "off" }),
                    false,
                );
            }
            Action::ToggleWordWrap => {
                self.config.word_wrap = !self.config.word_wrap;
                self.config.save();
                let on = self.config.word_wrap;
                self.editors[t]
                    .set_status(format!("Word wrap {}", if on { "on" } else { "off" }), false);
            }
            Action::ToggleAutoIndent => {
                self.config.auto_indent = !self.config.auto_indent;
                self.config.save();
                let on = self.config.auto_indent;
                self.editors[t].set_status(
                    format!("Auto-indent {}", if on { "on" } else { "off" }),
                    false,
                );
            }

            Action::OpenFile => self.handle_open_file()?,

            Action::NextTab => {
                if self.editors.len() > 1 {
                    self.active_tab = (self.active_tab + 1) % self.editors.len();
                }
            }
            Action::PrevTab => {
                if self.editors.len() > 1 {
                    self.active_tab =
                        (self.active_tab + self.editors.len() - 1) % self.editors.len();
                }
            }
            Action::CloseTab => self.handle_close_tab()?,
            Action::NewTab   => {
                self.editors.push(Editor::new(None));
                self.active_tab = self.editors.len() - 1;
            }
        }
        Ok(())
    }

    // ── Key event handler ─────────────────────────────────────────────────────

    fn handle_key(&mut self, ev: KeyEvent) -> io::Result<()> {
        // Ignore key-release events (some terminals emit them).
        if ev.kind == KeyEventKind::Release {
            return Ok(());
        }

        let ctrl  = ev.modifiers.contains(KeyModifiers::CONTROL);
        let alt   = ev.modifiers.contains(KeyModifiers::ALT);
        let shift = ev.modifiers.contains(KeyModifiers::SHIFT);

        // ── Navigation (with optional shift-selection) ────────────────────────
        //
        // For every nav key:
        //   • With Shift   → anchor at cursor if not set, then move (extends selection)
        //   • Without Shift → clear any selection, then move
        //
        // Helper closures (macros) are not possible here, so we handle each case
        // explicitly.  The pattern is always:
        //   1. if shift { set_anchor_if_none } else { clear_anchor }
        //   2. call the move method
        //   3. return Ok(())

        match ev.code {
            KeyCode::Up if !ctrl && !alt => {
                if shift { self.set_anchor_if_none(); } else { self.editors[self.active_tab].selection_anchor = None; }
                self.editors[self.active_tab].move_up();
                return Ok(());
            }
            KeyCode::Down if !ctrl && !alt => {
                if shift { self.set_anchor_if_none(); } else { self.editors[self.active_tab].selection_anchor = None; }
                self.editors[self.active_tab].move_down();
                return Ok(());
            }
            KeyCode::Left if !ctrl && !alt => {
                if shift { self.set_anchor_if_none(); } else { self.editors[self.active_tab].selection_anchor = None; }
                self.editors[self.active_tab].move_left();
                return Ok(());
            }
            KeyCode::Right if !ctrl && !alt => {
                if shift { self.set_anchor_if_none(); } else { self.editors[self.active_tab].selection_anchor = None; }
                self.editors[self.active_tab].move_right();
                return Ok(());
            }
            // Ctrl+Left / Ctrl+Right  (also handles Shift+Ctrl variants)
            KeyCode::Left if ctrl && !alt => {
                if shift { self.set_anchor_if_none(); } else { self.editors[self.active_tab].selection_anchor = None; }
                self.editors[self.active_tab].move_prev_word();
                return Ok(());
            }
            KeyCode::Right if ctrl && !alt => {
                if shift { self.set_anchor_if_none(); } else { self.editors[self.active_tab].selection_anchor = None; }
                self.editors[self.active_tab].move_next_word();
                return Ok(());
            }
            KeyCode::Home if !ctrl => {
                if shift { self.set_anchor_if_none(); } else { self.editors[self.active_tab].selection_anchor = None; }
                self.editors[self.active_tab].move_home();
                return Ok(());
            }
            KeyCode::End if !ctrl => {
                if shift { self.set_anchor_if_none(); } else { self.editors[self.active_tab].selection_anchor = None; }
                self.editors[self.active_tab].move_end();
                return Ok(());
            }
            KeyCode::Home if ctrl => {
                let t = self.active_tab;
                if shift { self.set_anchor_if_none(); } else { self.editors[t].selection_anchor = None; }
                self.editors[self.active_tab].cursor_row = 0;
                self.editors[self.active_tab].cursor_col = 0;
                return Ok(());
            }
            KeyCode::End if ctrl => {
                let t = self.active_tab;
                if shift { self.set_anchor_if_none(); } else { self.editors[t].selection_anchor = None; }
                let last = self.editors[t].lines.len().saturating_sub(1);
                let last_col = self.editors[t].lines.last().map(|l| l.len()).unwrap_or(0);
                self.editors[t].cursor_row = last;
                self.editors[t].cursor_col = last_col;
                return Ok(());
            }
            KeyCode::PageUp => {
                let t = self.active_tab;
                if shift { self.set_anchor_if_none(); } else { self.editors[t].selection_anchor = None; }
                let h = self.text_height();
                self.editors[t].move_page_up(h);
                return Ok(());
            }
            KeyCode::PageDown => {
                let t = self.active_tab;
                if shift { self.set_anchor_if_none(); } else { self.editors[t].selection_anchor = None; }
                let h = self.text_height();
                self.editors[t].move_page_down(h);
                return Ok(());
            }
            KeyCode::Enter => {
                let ai = self.config.auto_indent;
                self.editors[self.active_tab].insert_newline(ai);
                return Ok(());
            }
            KeyCode::Backspace if !ctrl && !alt => {
                self.editors[self.active_tab].backspace();
                return Ok(());
            }
            KeyCode::Delete if !ctrl && !alt => {
                self.editors[self.active_tab].delete_char();
                return Ok(());
            }
            KeyCode::Tab => {
                let spaces = self.config.use_spaces;
                let tab_size = self.config.tab_size;
                if spaces {
                    self.editors[self.active_tab].insert_str(&" ".repeat(tab_size));
                } else {
                    self.editors[self.active_tab].insert_char('\t');
                }
                return Ok(());
            }
            KeyCode::Esc => {
                // Clear selection on Escape (fall through to keybinding lookup
                // so a bound "escape" action still fires, but clear first).
                self.editors[self.active_tab].selection_anchor = None;
            }
            _ => {}
        }

        // ── Keybinding lookup ─────────────────────────────────────────────────
        let key_name = key_event_to_name(&ev);
        if let Some(action) = self.kb.get_action(&key_name) {
            return self.handle_action(action);
        }

        // ── Printable character fallthrough ───────────────────────────────────
        if let KeyCode::Char(c) = ev.code {
            if !ctrl && !alt {
                self.editors[self.active_tab].insert_char(c);
            }
        }

        Ok(())
    }

    /// Set `selection_anchor` to the current cursor position, but only if no
    /// anchor is already set (so extending an existing selection doesn't reset it).
    fn set_anchor_if_none(&mut self) {
        let t = self.active_tab;
        if self.editors[t].selection_anchor.is_none() {
            self.editors[t].selection_anchor =
                Some((self.editors[t].cursor_row, self.editors[t].cursor_col));
        }
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

            let th = self.text_height();
            let tw = self.text_width();
            self.editors[self.active_tab].update_scroll(th, tw);
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
        println!("Usage:  nanolike [OPTIONS] [FILE...]");
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

    let filenames: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect();

    let mut app = match App::new(filenames) {
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
