use std::collections::HashSet;
use std::io::{self, Write};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    queue,
    style::{self, Attribute, Color, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal,
};

use crate::config::{config_path, keybindings_path, Config, KeyBindings};
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
    editor: &Editor,
    config: &Config,
    kb: &KeyBindings,
) -> io::Result<()> {
    let (term_w, term_h) = terminal::size()?;
    let w = term_w as usize;
    let h = term_h as usize;

    if h < 4 || w < 10 {
        return Ok(());
    }

    let text_height = h - 3; // title row + status row + help row
    let lnw = line_number_width(editor, config);
    let text_width = w.saturating_sub(lnw);

    queue!(stdout, terminal::Clear(terminal::ClearType::All), cursor::Hide)?;

    draw_title_bar(stdout, editor, w)?;
    draw_text_area(stdout, editor, text_height, text_width, lnw, w)?;
    draw_status_bar(stdout, editor, h, w)?;
    draw_help_bar(stdout, kb, h, w)?;

    // Place the terminal cursor.
    let screen_row = editor.cursor_row.saturating_sub(editor.scroll_row) + 1;
    let char_col = editor.cursor_char_col();
    let screen_col = char_col.saturating_sub(editor.scroll_col) + lnw;
    if screen_row < h - 2 && screen_col < w {
        queue!(
            stdout,
            cursor::MoveTo(screen_col as u16, screen_row as u16),
            cursor::Show,
        )?;
    }

    stdout.flush()
}

fn draw_title_bar(stdout: &mut impl Write, editor: &Editor, w: usize) -> io::Result<()> {
    let name = editor.filename.as_deref().unwrap_or("New Buffer");
    let modified = if editor.modified { " [Modified]" } else { "" };
    let title = format!(" NanoLike \u{2014} {}{} ", name, modified);
    let padded = center_str(&title, w);
    queue!(
        stdout,
        cursor::MoveTo(0, 0),
        SetBackgroundColor(Color::Blue),
        SetForegroundColor(Color::White),
        SetAttribute(Attribute::Bold),
        style::Print(&padded),
        ResetColor,
    )
}

/// Builds the set of (row, byte_offset) pairs that should be highlighted as search matches.
fn build_search_highlight_set(editor: &Editor) -> HashSet<(usize, usize)> {
    let mut set = HashSet::new();
    if editor.search_term.is_empty() {
        return set;
    }
    for &(row, start_col) in &editor.search_matches {
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
) -> io::Result<()> {
    let line = &editor.lines[file_row];
    let visible: Vec<(usize, char)> = line
        .char_indices()
        .skip(editor.scroll_col)
        .take(text_width)
        .collect();

    let printed = visible.len();
    for (byte_pos, ch) in &visible {
        if search_set.contains(&(file_row, *byte_pos)) {
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
    let search_set = build_search_highlight_set(editor);

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
            render_line(stdout, editor, file_row, y, lnw, text_width, w, &search_set)?;
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
        cursor::MoveTo(0, (h - 2) as u16),
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
            cursor::MoveTo(0, (h - 2) as u16),
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
    let shortcuts = [
        ("quit", "Quit"),
        ("save", "Save"),
        ("search", "Search"),
        ("replace", "Replace"),
        ("goto_line", "GoTo"),
        ("cut_line", "Cut"),
        ("paste", "Paste"),
        ("help", "Help"),
        ("undo", "Undo"),
        ("redo", "Redo"),
    ];

    let mut bar = String::new();
    for (action, label) in &shortcuts {
        let key = kb.first_key(action);
        let entry = format!(" {} {} ", key, label);
        if bar.len() + entry.len() > w {
            break;
        }
        bar.push_str(&entry);
    }

    queue!(
        stdout,
        cursor::MoveTo(0, (h - 1) as u16),
        SetBackgroundColor(Color::Cyan),
        SetForegroundColor(Color::Black),
        style::Print(format!("{:<width$}", bar, width = w)),
        ResetColor,
    )
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
            cursor::MoveTo(0, (h - 2) as u16),
            SetBackgroundColor(Color::White),
            SetForegroundColor(Color::Black),
            SetAttribute(Attribute::Bold),
            style::Print(truncate_chars(&padded, w)),
            ResetColor,
            cursor::MoveTo((msg.len() + buf.len()).min(w.saturating_sub(1)) as u16, (h - 2) as u16),
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

    let mut sorted: Vec<(&String, &Vec<String>)> = kb.bindings.iter().collect();
    sorted.sort_by_key(|(k, _)| k.as_str());
    for (action, keys) in sorted {
        let label = action.replace('_', " ");
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
