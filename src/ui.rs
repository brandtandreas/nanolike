use std::collections::HashSet;
use std::io::{self, Write};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    queue,
    style::{self, Attribute, Color, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal,
};

use crate::config::{config_path, keybindings_path, Action, Config, KeyBindings};
use crate::editor::Editor;

/// Width (in columns) of the line-number gutter, or 0 if disabled.
pub fn line_number_width(editor: &Editor, config: &Config) -> usize {
    if config.line_numbers {
        editor.lines.len().to_string().len() + 1 // digits + one space
    } else {
        0
    }
}

/// Render the full UI to `stdout`.
pub fn render(
    stdout: &mut impl Write,
    editors: &[Editor],
    active_tab: usize,
    config: &Config,
    kb: &KeyBindings,
) -> io::Result<()> {
    let (term_w, term_h) = terminal::size()?;
    let w = term_w as usize;
    let h = term_h as usize;

    if h < 5 || w < 10 {
        return Ok(());
    }

    let editor = &editors[active_tab];
    let text_height = h - 4; // tab row + status row + 2 help rows
    let lnw = line_number_width(editor, config);
    let text_width = w.saturating_sub(lnw);

    queue!(stdout, terminal::Clear(terminal::ClearType::All), cursor::Hide)?;

    draw_tab_bar(stdout, editors, active_tab, w)?;
    draw_text_area(stdout, editor, text_height, text_width, lnw, w)?;
    draw_status_bar(stdout, editor, h, w)?;
    draw_help_bar(stdout, kb, h, w)?;

    // Place the terminal cursor.
    let screen_row = editor.cursor_row.saturating_sub(editor.scroll_row) + 1;
    let char_col = editor.cursor_char_col();
    let screen_col = char_col.saturating_sub(editor.scroll_col) + lnw;
    if screen_row < h - 3 && screen_col < w {
        queue!(
            stdout,
            cursor::MoveTo(screen_col as u16, screen_row as u16),
            cursor::Show,
        )?;
    }

    stdout.flush()
}

fn draw_tab_bar(
    stdout: &mut impl Write,
    editors: &[Editor],
    active_tab: usize,
    w: usize,
) -> io::Result<()> {
    // Fill entire row with the bar background first.
    queue!(
        stdout,
        cursor::MoveTo(0, 0),
        SetBackgroundColor(Color::DarkBlue),
        style::Print(format!("{:width$}", "", width = w)),
        cursor::MoveTo(0, 0),
    )?;

    let mut x = 0usize;

    for (i, editor) in editors.iter().enumerate() {
        let name = editor
            .filename
            .as_deref()
            .map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
            })
            .unwrap_or("New Buffer");

        let modified = editor.modified;
        // " *name " or "  name "
        let tab_text = format!(" {}{} ", if modified { "*" } else { " " }, name);
        let tab_w = tab_text.chars().count();

        // Stop drawing if we've run out of space (leave 1 col margin).
        if x + tab_w >= w {
            break;
        }

        if i == active_tab {
            queue!(
                stdout,
                SetBackgroundColor(Color::White),
                SetForegroundColor(Color::DarkBlue),
                SetAttribute(Attribute::Bold),
                style::Print(&tab_text),
                SetAttribute(Attribute::Reset),
                SetBackgroundColor(Color::DarkBlue),
            )?;
        } else {
            let fg = if modified { Color::Yellow } else { Color::Grey };
            queue!(
                stdout,
                SetBackgroundColor(Color::DarkBlue),
                SetForegroundColor(fg),
                style::Print(&tab_text),
            )?;
        }
        x += tab_w;

        // Separator between tabs.
        if i + 1 < editors.len() && x + 1 < w {
            queue!(
                stdout,
                SetBackgroundColor(Color::DarkBlue),
                SetForegroundColor(Color::DarkGrey),
                style::Print("\u{2502}"), // │
            )?;
            x += 1;
        }
    }

    // Right-align "NanoLike" brand if space allows.
    let brand = " NanoLike ";
    if w > x + brand.len() {
        queue!(
            stdout,
            cursor::MoveTo((w - brand.len()) as u16, 0),
            SetBackgroundColor(Color::DarkBlue),
            SetForegroundColor(Color::DarkGrey),
            style::Print(brand),
        )?;
    }

    queue!(stdout, ResetColor)
}

/// Builds the set of (row, byte_offset) pairs that fall inside the current selection,
/// restricted to the visible row range `[first_row, last_row]`.
fn build_selection_set(editor: &Editor, first_row: usize, last_row: usize) -> HashSet<(usize, usize)> {
    let mut set = HashSet::new();
    let Some(((sr, sc), (er, ec))) = editor.selection_ordered() else {
        return set;
    };
    let row_start = sr.max(first_row);
    let row_end   = er.min(last_row);
    for row in row_start..=row_end {
        if row >= editor.lines.len() {
            break;
        }
        let line = &editor.lines[row];
        let col_start = if row == sr { sc } else { 0 };
        let col_end   = if row == er { ec } else { line.len() };
        let mut byte_pos = col_start;
        while byte_pos < col_end {
            set.insert((row, byte_pos));
            byte_pos += line[byte_pos..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        }
    }
    set
}

/// Builds the set of (row, byte_offset) pairs that should be highlighted as search matches,
/// restricted to the visible row range `[first_row, last_row]`.
fn build_search_highlight_set(editor: &Editor, first_row: usize, last_row: usize) -> HashSet<(usize, usize)> {
    let mut set = HashSet::new();
    if editor.search_term.is_empty() {
        return set;
    }
    for &(row, start_col) in &editor.search_matches {
        if row < first_row { continue; }
        if row > last_row  { break; }
        if row >= editor.lines.len() {
            continue;
        }
        let line = &editor.lines[row];
        let mut byte_pos = start_col;
        for _ in editor.search_term.chars() {
            set.insert((row, byte_pos));
            match line[byte_pos..].chars().next() {
                Some(c) => byte_pos += c.len_utf8(),
                None => break,
            }
        }
    }
    set
}

fn render_line(
    stdout: &mut impl Write,
    editor: &Editor,
    file_row: usize,
    y: u16,
    lnw: usize,
    text_width: usize,
    w: usize,
    search_set: &HashSet<(usize, usize)>,
    sel_set: &HashSet<(usize, usize)>,
) -> io::Result<()> {
    let line = &editor.lines[file_row];
    let mut printed = 0usize;
    for (byte_pos, ch) in line.char_indices().skip(editor.scroll_col).take(text_width) {
        printed += 1;
        let in_sel    = sel_set.contains(&(file_row, byte_pos));
        let in_search = search_set.contains(&(file_row, byte_pos));
        if in_sel {
            queue!(
                stdout,
                SetBackgroundColor(Color::Cyan),
                SetForegroundColor(Color::Black),
                style::Print(ch),
                ResetColor,
            )?;
        } else if in_search {
            queue!(
                stdout,
                SetBackgroundColor(Color::Yellow),
                SetForegroundColor(Color::Black),
                SetAttribute(Attribute::Bold),
                style::Print(ch),
                ResetColor,
            )?;
        } else {
            queue!(stdout, style::Print(ch))?;
        }
    }

    // Pad the rest of the text area on this row.
    if printed < text_width {
        let pad_end = (lnw + text_width).min(w);
        let current_x = (lnw + printed) as u16;
        if (pad_end as u16) > current_x {
            queue!(
                stdout,
                cursor::MoveTo(current_x, y),
                style::Print(" ".repeat(pad_end - lnw - printed)),
            )?;
        }
    }
    Ok(())
}

fn draw_text_area(
    stdout: &mut impl Write,
    editor: &Editor,
    text_height: usize,
    text_width: usize,
    lnw: usize,
    w: usize,
) -> io::Result<()> {
    let first_row  = editor.scroll_row;
    let last_row   = editor.scroll_row.saturating_add(text_height).saturating_sub(1);
    let search_set = build_search_highlight_set(editor, first_row, last_row);
    let sel_set    = build_selection_set(editor, first_row, last_row);

    for screen_row in 0..text_height {
        let file_row = screen_row + editor.scroll_row;
        let y = (screen_row + 1) as u16;

        queue!(stdout, cursor::MoveTo(0, y))?;

        // Line number gutter
        if lnw > 0 {
            if file_row < editor.lines.len() {
                let lnum = format!("{:>width$} ", file_row + 1, width = lnw - 1);
                let lnum = truncate_chars(&lnum, lnw);
                queue!(
                    stdout,
                    SetForegroundColor(Color::DarkYellow),
                    style::Print(&lnum),
                    ResetColor,
                )?;
            } else {
                queue!(stdout, style::Print(" ".repeat(lnw)))?;
            }
        }

        if file_row < editor.lines.len() {
            render_line(stdout, editor, file_row, y, lnw, text_width, w, &search_set, &sel_set)?;
        } else {
            // Past the end of the file: show a tilde.
            queue!(
                stdout,
                SetForegroundColor(Color::DarkGrey),
                style::Print("~"),
                ResetColor,
            )?;
            let remaining = w.saturating_sub(lnw + 1);
            if remaining > 0 {
                queue!(stdout, style::Print(" ".repeat(remaining)))?;
            }
        }
    }
    Ok(())
}

fn draw_status_bar(stdout: &mut impl Write, editor: &Editor, h: usize, w: usize) -> io::Result<()> {
    let bom_tag = if editor.has_bom { " BOM" } else { "" };
    let col_info = format!(
        " Ln {}, Col {} | {}{} ",
        editor.cursor_row + 1,
        editor.cursor_char_col() + 1,
        editor.encoding.name(),
        bom_tag,
    );
    let msg_width = w.saturating_sub(col_info.len());

    let msg = if !editor.status_msg.is_empty() {
        format!(" {}", editor.status_msg)
    } else {
        String::new()
    };
    let msg_display = truncate_chars(&msg, msg_width);
    let padded_msg = format!("{:<width$}", msg_display, width = msg_width);

    let full_line = format!("{}{}", padded_msg, col_info);

    queue!(
        stdout,
        cursor::MoveTo(0, (h - 3) as u16),
        SetBackgroundColor(Color::White),
        SetForegroundColor(Color::Black),
        style::Print(truncate_chars(&full_line, w)),
        ResetColor,
    )?;

    // Overlay status message in a distinct colour if present.
    if !editor.status_msg.is_empty() {
        let msg_col = if editor.status_error {
            Color::Red
        } else {
            Color::DarkGreen
        };
        let display = truncate_chars(&msg, msg_width);
        queue!(
            stdout,
            cursor::MoveTo(0, (h - 3) as u16),
            SetBackgroundColor(Color::White),
            SetForegroundColor(msg_col),
            SetAttribute(Attribute::Bold),
            style::Print(&display),
            ResetColor,
        )?;
    }
    Ok(())
}

fn draw_help_bar(stdout: &mut impl Write, kb: &KeyBindings, h: usize, w: usize) -> io::Result<()> {
    let row1: &[(Action, &str)] = &[
        (Action::Quit,     "Quit"),
        (Action::Save,     "Save"),
        (Action::Search,   "Search"),
        (Action::Replace,  "Replace"),
        (Action::GotoLine, "GoTo"),
        (Action::Undo,     "Undo"),
    ];
    let row2: &[(Action, &str)] = &[
        (Action::CutLine,   "Cut"),
        (Action::CopyLine,  "Copy"),
        (Action::Paste,     "Paste"),
        (Action::SelectAll, "Sel All"),
        (Action::Redo,      "Redo"),
        (Action::OpenFile,  "Open"),
        (Action::Help,      "Help"),
    ];

    // Compute per-column widths so both rows align.
    let ncols = row1.len().max(row2.len());
    let entry_width = |action: Action, label: &str| {
        kb.first_key(action).chars().count() + 1 + label.chars().count()
    };
    let col_widths: Vec<usize> = (0..ncols)
        .map(|i| {
            let w1 = row1.get(i).map(|(a, l)| entry_width(*a, l)).unwrap_or(0);
            let w2 = row2.get(i).map(|(a, l)| entry_width(*a, l)).unwrap_or(0);
            w1.max(w2)
        })
        .collect();

    draw_help_row(stdout, row1, kb, (h - 2) as u16, w, &col_widths)?;
    draw_help_row(stdout, row2, kb, (h - 1) as u16, w, &col_widths)
}

fn draw_help_row(
    stdout: &mut impl Write,
    shortcuts: &[(Action, &str)],
    kb: &KeyBindings,
    y: u16,
    w: usize,
    col_widths: &[usize],
) -> io::Result<()> {
    const BG: Color = Color::DarkBlue;
    const SEP: &str = "  \u{2502}  "; // "  │  "

    // Fill the row with the background colour.
    queue!(
        stdout,
        cursor::MoveTo(0, y),
        SetBackgroundColor(BG),
        style::Print(format!("{:width$}", "", width = w)),
        cursor::MoveTo(0, y),
    )?;

    let mut x = 0usize;
    let mut first = true;

    for (i, (action, label)) in shortcuts.iter().enumerate() {
        let key = kb.first_key(*action);
        let col_w = col_widths.get(i).copied().unwrap_or(0);
        let entry_w = key.chars().count() + 1 + label.chars().count();
        let pad = col_w.saturating_sub(entry_w);

        // Separator between entries.
        let sep = if first { " " } else { SEP };
        if x + sep.chars().count() + col_w >= w {
            break;
        }

        // Separator / leading space.
        queue!(
            stdout,
            SetBackgroundColor(BG),
            SetForegroundColor(Color::DarkCyan),
            style::Print(sep),
        )?;
        x += sep.chars().count();
        first = false;

        // Key: bold white.
        queue!(
            stdout,
            SetBackgroundColor(BG),
            SetForegroundColor(Color::White),
            SetAttribute(Attribute::Bold),
            style::Print(key),
            SetAttribute(Attribute::Reset),
        )?;
        x += key.chars().count();

        // Label: normal cyan (re-apply BG after attribute reset).
        queue!(
            stdout,
            SetBackgroundColor(BG),
            SetForegroundColor(Color::Cyan),
            style::Print(format!(" {}", label)),
        )?;
        x += 1 + label.chars().count();

        // Trailing padding so this column occupies exactly col_w chars.
        if pad > 0 {
            queue!(
                stdout,
                SetBackgroundColor(BG),
                style::Print(" ".repeat(pad)),
            )?;
            x += pad;
        }
    }

    queue!(stdout, ResetColor)
}

// ── File-path completion ──────────────────────────────────────────────────────

const COMPLETE_MAX: usize = 8;

/// Expand a leading `~` using the `HOME` environment variable.
fn expand_tilde(s: &str) -> String {
    if s == "~" || s.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return s.replacen('~', &home, 1);
        }
    }
    s.to_string()
}

/// Return sorted filesystem completions for a partial path string.
fn get_path_completions(partial: &str) -> Vec<String> {
    // Split the input into the directory to scan and the filename prefix.
    let (scan_dir, name_prefix, out_prefix) = if partial.is_empty() {
        (".".to_string(), String::new(), String::new())
    } else if partial.ends_with('/') {
        // "src/" → scan "src", prefix every result with "src/"
        let dir = partial.trim_end_matches('/');
        let dir = if dir.is_empty() { "/" } else { dir };
        (dir.to_string(), String::new(), partial.to_string())
    } else {
        let p = std::path::Path::new(partial);
        let parent = p.parent().unwrap_or(std::path::Path::new(""));
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let (scan, pfx) = if parent == std::path::Path::new("") {
            (".".to_string(), String::new())
        } else {
            (
                parent.to_string_lossy().to_string(),
                format!("{}/", parent.display()),
            )
        };
        (scan, name, pfx)
    };

    let scan_expanded = expand_tilde(&scan_dir);
    let Ok(entries) = std::fs::read_dir(&scan_expanded) else {
        return vec![];
    };

    let mut results: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let fname = e.file_name();
            let fname_str = fname.to_string_lossy();
            if fname_str.starts_with(&name_prefix as &str) {
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let mut r = format!("{}{}", out_prefix, fname_str);
                if is_dir {
                    r.push('/');
                }
                Some(r)
            } else {
                None
            }
        })
        .collect();

    results.sort();
    results
}

/// Longest string that is a prefix of every element in `strs`.
fn common_prefix(strs: &[String]) -> String {
    if strs.is_empty() {
        return String::new();
    }
    let mut prefix = strs[0].clone();
    for s in &strs[1..] {
        let len: usize = prefix
            .chars()
            .zip(s.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum();
        prefix.truncate(len);
    }
    prefix
}

/// How many screen rows the popup for `n` completions occupies
/// (includes the counter line when n > COMPLETE_MAX).
fn popup_rows(n: usize) -> usize {
    let shown = n.min(COMPLETE_MAX);
    if n > COMPLETE_MAX { shown + 1 } else { shown }
}

/// Draw the completion popup above `prompt_row`.
/// Returns the number of rows drawn.
fn draw_completion_popup(
    stdout: &mut impl Write,
    completions: &[String],
    selected: isize, // -1 = none, 0.. = index
    prompt_row: usize,
    w: usize,
) -> io::Result<usize> {
    let n = completions.len();
    if n == 0 {
        return Ok(0);
    }
    let shown = n.min(COMPLETE_MAX);
    let has_counter = n > COMPLETE_MAX;
    let total = popup_rows(n);

    // Scroll window: keep `selected` visible.
    let offset = if selected >= 0 {
        let s = selected as usize;
        s.saturating_sub(shown - 1).min(n.saturating_sub(shown))
    } else {
        0
    };

    // Draw entries (top of popup first).
    for i in 0..shown {
        let item_idx = offset + i;
        let row = (prompt_row as isize - total as isize + i as isize).max(1) as u16;
        let is_sel = selected == item_idx as isize;
        let label = format!(" {} ", &completions[item_idx]);
        let padded = format!("{:<width$}", truncate_chars(&label, w), width = w);
        queue!(
            stdout,
            cursor::MoveTo(0, row),
            SetBackgroundColor(if is_sel { Color::Blue } else { Color::DarkGrey }),
            SetForegroundColor(Color::White),
            style::Print(&padded),
            ResetColor,
        )?;
    }

    // Counter line at the bottom of the popup when there are hidden items.
    if has_counter {
        let row = (prompt_row as isize - 1).max(1) as u16;
        let msg = format!("  {}/{} matches", offset + shown, n);
        let padded = format!("{:<width$}", truncate_chars(&msg, w), width = w);
        queue!(
            stdout,
            cursor::MoveTo(0, row),
            SetBackgroundColor(Color::DarkGrey),
            SetForegroundColor(Color::DarkCyan),
            style::Print(&padded),
            ResetColor,
        )?;
    }

    Ok(total)
}

// ── Interactive prompt ────────────────────────────────────────────────────────

/// Show a prompt on the status row and collect a line of input.
/// Returns `Some(input)` on Enter, `None` on Escape.
pub fn prompt(
    stdout: &mut impl Write,
    msg: &str,
    default: &str,
    h: usize,
    w: usize,
) -> io::Result<Option<String>> {
    let mut buf = default.to_string();

    loop {
        let display = format!("{}{}", msg, buf);
        let padded = format!("{:<width$}", display, width = w);
        queue!(
            stdout,
            cursor::MoveTo(0, (h - 3) as u16),
            SetBackgroundColor(Color::White),
            SetForegroundColor(Color::Black),
            SetAttribute(Attribute::Bold),
            style::Print(truncate_chars(&padded, w)),
            ResetColor,
            cursor::MoveTo((msg.len() + buf.len()).min(w.saturating_sub(1)) as u16, (h - 3) as u16),
            cursor::Show,
        )?;
        stdout.flush()?;

        let Event::Key(key) = event::read()? else { continue };
        match key.code {
            KeyCode::Enter => return Ok(Some(buf)),
            KeyCode::Esc => return Ok(None),
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                buf.push(c);
            }
            _ => {}
        }
    }
}

/// File-path prompt with Tab completion.
///
/// - **Tab** completes the longest common prefix; a second Tab opens the popup
///   and cycles forward through matches.
/// - **Shift+Tab** cycles backward.
/// - Typing any character resets the completion state.
pub fn file_prompt(
    stdout: &mut impl Write,
    msg: &str,
    default: &str,
    h: usize,
    w: usize,
) -> io::Result<Option<String>> {
    let mut buf = default.to_string();
    let mut completions: Vec<String> = vec![];
    let mut lcp = String::new();
    let mut cycle: isize = -1; // -1 = at lcp, 0..= = specific index
    let mut show_popup = false;
    let mut prev_popup_rows = 0usize;
    let prompt_row = h - 3;

    loop {
        // ── Clear the popup area from the previous frame ───────────────────
        let cur_rows = if show_popup && !completions.is_empty() {
            popup_rows(completions.len())
        } else {
            0
        };
        let clear_rows = prev_popup_rows.max(cur_rows);
        for i in 0..clear_rows {
            let row = (prompt_row as isize - clear_rows as isize + i as isize).max(1) as u16;
            queue!(
                stdout,
                cursor::MoveTo(0, row),
                ResetColor,
                style::Print(" ".repeat(w)),
            )?;
        }

        // ── Draw prompt row ────────────────────────────────────────────────
        let display = format!("{}{}", msg, buf);
        let padded = format!("{:<width$}", display, width = w);
        let cursor_x = (msg.chars().count() + buf.chars().count()).min(w.saturating_sub(1));
        queue!(
            stdout,
            cursor::MoveTo(0, prompt_row as u16),
            SetBackgroundColor(Color::White),
            SetForegroundColor(Color::Black),
            SetAttribute(Attribute::Bold),
            style::Print(truncate_chars(&padded, w)),
            ResetColor,
            cursor::MoveTo(cursor_x as u16, prompt_row as u16),
            cursor::Show,
        )?;

        // ── Draw completion popup ──────────────────────────────────────────
        prev_popup_rows = if show_popup && !completions.is_empty() {
            draw_completion_popup(stdout, &completions, cycle, prompt_row, w)?
        } else {
            0
        };

        stdout.flush()?;

        // ── Handle input ───────────────────────────────────────────────────
        let Event::Key(key) = event::read()? else { continue };
        if key.kind == event::KeyEventKind::Release {
            continue;
        }

        match key.code {
            KeyCode::Enter => return Ok(Some(buf)),
            KeyCode::Esc   => return Ok(None),

            KeyCode::Tab => {
                if completions.is_empty() {
                    // First Tab: compute completions.
                    completions = get_path_completions(&buf);
                    if completions.is_empty() {
                        // no matches — nothing to do
                    } else if completions.len() == 1 {
                        buf = completions.remove(0);
                        completions.clear();
                    } else {
                        lcp = common_prefix(&completions);
                        if lcp.len() > buf.len() {
                            // Advance to LCP but don't show popup yet.
                            buf = lcp.clone();
                        } else {
                            // Already at LCP — show popup immediately.
                            show_popup = true;
                            cycle = -1;
                        }
                    }
                } else {
                    // Subsequent Tab: cycle forward through matches.
                    show_popup = true;
                    cycle += 1;
                    if cycle >= completions.len() as isize {
                        cycle = -1;
                        buf = lcp.clone();
                    } else {
                        buf = completions[cycle as usize].clone();
                    }
                }
            }

            // Shift+Tab: cycle backward.
            KeyCode::BackTab => {
                if !completions.is_empty() {
                    show_popup = true;
                    cycle -= 1;
                    if cycle < -1 {
                        cycle = completions.len() as isize - 1;
                    }
                    buf = if cycle < 0 {
                        lcp.clone()
                    } else {
                        completions[cycle as usize].clone()
                    };
                }
            }

            KeyCode::Backspace => {
                buf.pop();
                completions.clear();
                show_popup = false;
                cycle = -1;
            }

            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                buf.push(c);
                completions.clear();
                show_popup = false;
                cycle = -1;
            }

            _ => {}
        }
    }
}

// ── Help screen ───────────────────────────────────────────────────────────────

pub fn show_help(
    stdout: &mut impl Write,
    kb: &KeyBindings,
    h: usize,
    w: usize,
) -> io::Result<()> {
    queue!(
        stdout,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0),
        SetBackgroundColor(Color::Blue),
        SetForegroundColor(Color::White),
        SetAttribute(Attribute::Bold),
        style::Print(center_str(" NanoLike Help ", w)),
        ResetColor,
    )?;

    let mut lines: Vec<String> = vec![
        String::new(),
        "Keyboard Shortcuts:".to_string(),
        "\u{2500}".repeat(w.min(50)),
        String::new(),
    ];

    let mut sorted: Vec<(&Action, &Vec<String>)> = kb.bindings.iter().collect();
    sorted.sort_by_key(|(k, _)| k.to_string());
    for (action, keys) in sorted {
        let label = action.to_string().replace('_', " ");
        let keys_str = keys.join(" / ");
        lines.push(format!("  {:<22} {}", keys_str, label));
    }

    lines.push(String::new());
    lines.push(format!("Config:      {}", config_path().display()));
    lines.push(format!("Keybindings: {}", keybindings_path().display()));
    lines.push(String::new());

    for (i, line) in lines.iter().enumerate().take(h.saturating_sub(3)) {
        queue!(
            stdout,
            cursor::MoveTo(0, (i + 1) as u16),
            style::Print(format!("{:<width$}", truncate_chars(line, w), width = w)),
        )?;
    }

    queue!(
        stdout,
        cursor::MoveTo(0, (h - 1) as u16),
        SetBackgroundColor(Color::Cyan),
        SetForegroundColor(Color::Black),
        style::Print(format!("{:<width$}", " Press any key to close ", width = w)),
        ResetColor,
        cursor::Show,
    )?;
    stdout.flush()?;

    loop {
        if let Event::Key(_) = event::read()? {
            break;
        }
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn truncate_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

fn center_str(s: &str, width: usize) -> String {
    let s_chars = s.chars().count();
    if s_chars >= width {
        return truncate_chars(s, width);
    }
    let total_pad = width - s_chars;
    let left = total_pad / 2;
    let right = total_pad - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}
