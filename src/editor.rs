use std::collections::VecDeque;

use encoding_rs::Encoding;

const MAX_UNDO: usize = 200;

#[derive(Clone)]
pub struct Snapshot {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

pub struct UndoStack {
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        }
    }

    /// Call before making any change to record the pre-change state.
    pub fn push_edit(&mut self, snap: Snapshot) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.pop_front();
        }
        self.undo.push_back(snap);
        self.redo.clear();
    }

    /// Undo: returns the state to restore, pushes current state onto redo.
    pub fn do_undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let prev = self.undo.pop_back()?;
        if self.redo.len() >= MAX_UNDO {
            self.redo.pop_front();
        }
        self.redo.push_back(current);
        Some(prev)
    }

    /// Redo: returns the state to restore, pushes current state onto undo.
    pub fn do_redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop_back()?;
        if self.undo.len() >= MAX_UNDO {
            self.undo.pop_front();
        }
        self.undo.push_back(current);
        Some(next)
    }

    pub fn _can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn _can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

pub struct Editor {
    pub lines: Vec<String>,
    /// Byte offset of the cursor in the current line.
    pub cursor_row: usize,
    pub cursor_col: usize,
    /// First visible row / first visible char-column.
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub filename: Option<String>,
    pub modified: bool,
    pub status_msg: String,
    pub status_error: bool,
    pub clipboard: Vec<String>,
    pub undo_stack: UndoStack,
    pub saved_lines: Vec<String>,
    pub search_term: String,
    /// (row, byte_col) of each match start.
    pub search_matches: Vec<(usize, usize)>,
    pub search_idx: usize,
    /// The encoding detected when the file was opened (default UTF-8 for new files).
    pub encoding: &'static Encoding,
    /// Whether the file had a byte-order mark (BOM); preserved on save.
    pub has_bom: bool,
}

impl Editor {
    pub fn new(filename: Option<String>) -> Self {
        let mut ed = Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            filename: None,
            modified: false,
            status_msg: String::new(),
            status_error: false,
            clipboard: Vec::new(),
            undo_stack: UndoStack::new(),
            saved_lines: vec![String::new()],
            search_term: String::new(),
            search_matches: Vec::new(),
            search_idx: 0,
            encoding: encoding_rs::UTF_8,
            has_bom: false,
        };
        if let Some(ref fname) = filename {
            if std::path::Path::new(fname).exists() {
                ed.load_file(fname);
            } else {
                ed.status_msg = format!("New file: {}", fname);
            }
            ed.filename = filename;
        }
        ed.saved_lines = ed.lines.clone();
        ed
    }

    fn load_file(&mut self, path: &str) {
        match std::fs::read(path) {
            Ok(bytes) => {
                // BOM detection has priority; fall back to statistical detection.
                let (encoding, bom_len) =
                    if let Some((enc, bom_len)) = Encoding::for_bom(&bytes) {
                        (enc, bom_len)
                    } else {
                        let mut detector = chardetng::EncodingDetector::new();
                        detector.feed(&bytes, true);
                        (detector.guess(None, true), 0)
                    };

                self.encoding = encoding;
                self.has_bom = bom_len > 0;

                let (cow, _, had_errors) = encoding.decode(&bytes[bom_len..]);
                self.lines = cow.lines().map(|l| l.to_string()).collect();
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }

                let enc_name = encoding.name();
                if had_errors {
                    self.set_status(
                        format!(
                            "Read {} lines [{}] (encoding errors; some chars replaced)",
                            self.lines.len(),
                            enc_name
                        ),
                        true,
                    );
                } else {
                    self.set_status(
                        format!("Read {} lines [{}]", self.lines.len(), enc_name),
                        false,
                    );
                }
            }
            Err(e) => {
                self.set_status(format!("Error: {}", e), true);
            }
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>, error: bool) {
        self.status_msg = msg.into();
        self.status_error = error;
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        }
    }

    fn save_undo(&mut self) {
        let snap = self.snapshot();
        self.undo_stack.push_edit(snap);
    }

    fn apply_snapshot(&mut self, snap: Snapshot) {
        self.lines = snap.lines;
        self.cursor_row = snap.cursor_row;
        self.cursor_col = snap.cursor_col;
        self.clamp_cursor();
        self.modified = self.lines != self.saved_lines;
    }

    pub fn do_undo(&mut self) {
        let current = self.snapshot();
        match self.undo_stack.do_undo(current) {
            Some(prev) => {
                self.apply_snapshot(prev);
                self.set_status("Undone", false);
            }
            None => self.set_status("Nothing to undo", false),
        }
    }

    pub fn do_redo(&mut self) {
        let current = self.snapshot();
        match self.undo_stack.do_redo(current) {
            Some(next) => {
                self.apply_snapshot(next);
                self.set_status("Redone", false);
            }
            None => self.set_status("Nothing to redo", false),
        }
    }

    pub fn clamp_cursor(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor_row = self.cursor_row.min(self.lines.len() - 1);
        let line_len = self.lines[self.cursor_row].len();
        self.cursor_col = self.cursor_col.min(line_len);
        // Snap back to a valid UTF-8 char boundary.
        while self.cursor_col > 0
            && !self.lines[self.cursor_row].is_char_boundary(self.cursor_col)
        {
            self.cursor_col -= 1;
        }
    }

    /// Number of chars (not bytes) before the cursor on the current line.
    pub fn cursor_char_col(&self) -> usize {
        self.lines[self.cursor_row][..self.cursor_col]
            .chars()
            .count()
    }

    // ── Editing ──────────────────────────────────────────────────────────────

    pub fn insert_char(&mut self, ch: char) {
        self.save_undo();
        let col = self.cursor_col;
        self.lines[self.cursor_row].insert(col, ch);
        self.cursor_col += ch.len_utf8();
        self.modified = true;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.save_undo();
        let col = self.cursor_col;
        self.lines[self.cursor_row].insert_str(col, s);
        self.cursor_col += s.len();
        self.modified = true;
    }

    pub fn insert_newline(&mut self, auto_indent: bool) {
        self.save_undo();
        let col = self.cursor_col;
        let after = self.lines[self.cursor_row].split_off(col);
        let indent: String = if auto_indent {
            self.lines[self.cursor_row]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect()
        } else {
            String::new()
        };
        let new_line = indent.clone() + &after;
        self.lines.insert(self.cursor_row + 1, new_line);
        self.cursor_row += 1;
        self.cursor_col = indent.len();
        self.modified = true;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.save_undo();
            let col = self.cursor_col;
            let prev_col = self.lines[self.cursor_row][..col]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.lines[self.cursor_row].drain(prev_col..col);
            self.cursor_col = prev_col;
            self.modified = true;
        } else if self.cursor_row > 0 {
            self.save_undo();
            let curr = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            let prev_len = self.lines[self.cursor_row].len();
            self.cursor_col = prev_len;
            self.lines[self.cursor_row].push_str(&curr);
            self.modified = true;
        }
    }

    pub fn delete_char(&mut self) {
        let line_len = self.lines[self.cursor_row].len();
        if self.cursor_col < line_len {
            self.save_undo();
            let col = self.cursor_col;
            let next = self.lines[self.cursor_row][col..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| col + i)
                .unwrap_or(line_len);
            self.lines[self.cursor_row].drain(col..next);
            self.modified = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.save_undo();
            let next_line = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next_line);
            self.modified = true;
        }
    }

    pub fn cut_line(&mut self) {
        self.save_undo();
        self.clipboard = vec![self.lines.remove(self.cursor_row)];
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.clamp_cursor();
        self.modified = true;
        self.set_status("Line cut", false);
    }

    pub fn copy_line(&mut self) {
        self.clipboard = vec![self.lines[self.cursor_row].clone()];
        self.set_status("Line copied", false);
    }

    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            self.set_status("Clipboard empty", false);
            return;
        }
        self.save_undo();
        let clips = self.clipboard.clone();
        let count = clips.len();
        for (i, line) in clips.into_iter().enumerate() {
            self.lines.insert(self.cursor_row + i + 1, line);
        }
        self.cursor_row += count;
        self.modified = true;
        self.set_status(format!("Pasted {} line(s)", count), false);
    }

    pub fn delete_word_before(&mut self) {
        if self.cursor_col == 0 {
            return;
        }
        self.save_undo();
        let col = self.cursor_col;
        let line = self.lines[self.cursor_row].clone();
        let before = &line[..col];
        let trimmed = before.trim_end_matches(char::is_whitespace);
        let new_end = if trimmed.len() < before.len() {
            trimmed.len()
        } else {
            trimmed
                .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_')
                .len()
        };
        self.lines[self.cursor_row] = line[..new_end].to_string() + &line[col..];
        self.cursor_col = new_end;
        self.modified = true;
    }

    pub fn delete_to_eol(&mut self) {
        self.save_undo();
        let col = self.cursor_col;
        self.lines[self.cursor_row].truncate(col);
        self.modified = true;
    }

    // ── Navigation ───────────────────────────────────────────────────────────

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            let col = self.cursor_col;
            self.cursor_col = self.lines[self.cursor_row][..col]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    pub fn move_right(&mut self) {
        let line = &self.lines[self.cursor_row];
        if self.cursor_col < line.len() {
            let col = self.cursor_col;
            self.cursor_col = line[col..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| col + i)
                .unwrap_or(line.len());
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            let char_col = self.cursor_char_col();
            self.cursor_row -= 1;
            self.cursor_col = char_to_byte(&self.lines[self.cursor_row], char_col);
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            let char_col = self.cursor_char_col();
            self.cursor_row += 1;
            self.cursor_col = char_to_byte(&self.lines[self.cursor_row], char_col);
        }
    }

    pub fn move_home(&mut self) {
        let indent = self.lines[self.cursor_row]
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| c.len_utf8())
            .sum::<usize>();
        self.cursor_col = if self.cursor_col != indent { indent } else { 0 };
    }

    pub fn move_end(&mut self) {
        self.cursor_col = self.lines[self.cursor_row].len();
    }

    pub fn move_next_word(&mut self) {
        let chars: Vec<(usize, char)> = self.lines[self.cursor_row].char_indices().collect();
        let mut idx = chars
            .iter()
            .position(|&(b, _)| b >= self.cursor_col)
            .unwrap_or(chars.len());
        while idx < chars.len() && (chars[idx].1.is_alphanumeric() || chars[idx].1 == '_') {
            idx += 1;
        }
        while idx < chars.len() && !(chars[idx].1.is_alphanumeric() || chars[idx].1 == '_') {
            idx += 1;
        }
        let line_len = self.lines[self.cursor_row].len();
        self.cursor_col = chars.get(idx).map(|&(b, _)| b).unwrap_or(line_len);
    }

    pub fn move_prev_word(&mut self) {
        let chars: Vec<(usize, char)> = self.lines[self.cursor_row].char_indices().collect();
        let mut idx = chars
            .iter()
            .rposition(|&(b, _)| b < self.cursor_col)
            .map(|i| i + 1)
            .unwrap_or(0);
        while idx > 0 && !(chars[idx - 1].1.is_alphanumeric() || chars[idx - 1].1 == '_') {
            idx -= 1;
        }
        while idx > 0 && (chars[idx - 1].1.is_alphanumeric() || chars[idx - 1].1 == '_') {
            idx -= 1;
        }
        self.cursor_col = chars.get(idx).map(|&(b, _)| b).unwrap_or(0);
    }

    pub fn move_page_up(&mut self, page_height: usize) {
        let char_col = self.cursor_char_col();
        self.cursor_row = self.cursor_row.saturating_sub(page_height);
        self.cursor_col = char_to_byte(&self.lines[self.cursor_row], char_col);
    }

    pub fn move_page_down(&mut self, page_height: usize) {
        let char_col = self.cursor_char_col();
        self.cursor_row = (self.cursor_row + page_height).min(self.lines.len() - 1);
        self.cursor_col = char_to_byte(&self.lines[self.cursor_row], char_col);
    }

    pub fn goto_line(&mut self, n: usize) {
        self.cursor_row = n.min(self.lines.len() - 1);
        self.cursor_col = 0;
    }

    // ── File I/O ─────────────────────────────────────────────────────────────

    pub fn save_file(&mut self, path: &str) -> bool {
        let content = self.lines.join("\n") + "\n";
        let bytes = self.encode_content(&content);
        match std::fs::write(path, bytes) {
            Ok(_) => {
                self.filename = Some(path.to_string());
                self.modified = false;
                self.saved_lines = self.lines.clone();
                self.set_status(
                    format!("Saved: {} [{}]", path, self.encoding.name()),
                    false,
                );
                true
            }
            Err(e) => {
                self.set_status(format!("Save error: {}", e), true);
                false
            }
        }
    }

    /// Encode `content` (internal UTF-8) back to the file's original encoding.
    /// UTF-16 LE/BE are handled manually since encoding_rs only decodes them.
    fn encode_content(&self, content: &str) -> Vec<u8> {
        if self.encoding == encoding_rs::UTF_16LE {
            let mut bytes = if self.has_bom { vec![0xFF_u8, 0xFE] } else { Vec::new() };
            for unit in content.encode_utf16() {
                bytes.push(unit as u8);
                bytes.push((unit >> 8) as u8);
            }
            return bytes;
        }
        if self.encoding == encoding_rs::UTF_16BE {
            let mut bytes = if self.has_bom { vec![0xFE_u8, 0xFF] } else { Vec::new() };
            for unit in content.encode_utf16() {
                bytes.push((unit >> 8) as u8);
                bytes.push(unit as u8);
            }
            return bytes;
        }

        // All other encodings (UTF-8, Latin-1, Windows-125x, GBK, …)
        let (cow, _, _) = self.encoding.encode(content);
        let encoded = cow.into_owned();

        // Re-prepend a UTF-8 BOM if the original file had one.
        if self.has_bom && self.encoding == encoding_rs::UTF_8 {
            let mut result = Vec::with_capacity(3 + encoded.len());
            result.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            result.extend_from_slice(&encoded);
            return result;
        }

        encoded
    }

    // ── Search ────────────────────────────────────────────────────────────────

    pub fn build_search_matches(&mut self) {
        self.search_matches.clear();
        if self.search_term.is_empty() {
            return;
        }
        let term_lower = self.search_term.to_lowercase();
        for (row, line) in self.lines.iter().enumerate() {
            let line_lower = line.to_lowercase();
            for col in find_matches_in_line(&line_lower, &term_lower) {
                self.search_matches.push((row, col));
            }
        }
    }

    pub fn search_next(&mut self) -> bool {
        if self.search_matches.is_empty() {
            return false;
        }
        // Start searching from just past the current cursor position.
        let pos = (self.cursor_row, self.cursor_col.saturating_add(1));
        for (i, &(row, col)) in self.search_matches.iter().enumerate() {
            if (row, col) >= pos {
                self.search_idx = i;
                self.cursor_row = row;
                self.cursor_col = col;
                self.set_status(
                    format!("Match {}/{}: {}", i + 1, self.search_matches.len(), self.search_term),
                    false,
                );
                return true;
            }
        }
        // Wrap around.
        let (row, col) = self.search_matches[0];
        self.search_idx = 0;
        self.cursor_row = row;
        self.cursor_col = col;
        self.set_status(
            format!("Wrapped. Match 1/{}: {}", self.search_matches.len(), self.search_term),
            false,
        );
        true
    }

    pub fn search_prev(&mut self) -> bool {
        if self.search_matches.is_empty() {
            return false;
        }
        let pos = (self.cursor_row, self.cursor_col);
        for i in (0..self.search_matches.len()).rev() {
            let (row, col) = self.search_matches[i];
            if (row, col) < pos {
                self.search_idx = i;
                self.cursor_row = row;
                self.cursor_col = col;
                self.set_status(
                    format!(
                        "Match {}/{}: {}",
                        i + 1,
                        self.search_matches.len(),
                        self.search_term
                    ),
                    false,
                );
                return true;
            }
        }
        // Wrap around.
        let last = self.search_matches.len() - 1;
        let (row, col) = self.search_matches[last];
        self.search_idx = last;
        self.cursor_row = row;
        self.cursor_col = col;
        self.set_status(
            format!(
                "Wrapped. Match {n}/{n}: {t}",
                n = self.search_matches.len(),
                t = self.search_term
            ),
            false,
        );
        true
    }

    pub fn replace_all(&mut self, term: &str, replacement: &str) -> usize {
        if term.is_empty() {
            return 0;
        }
        self.save_undo();
        let mut count = 0usize;
        for line in &mut self.lines {
            let occurrences = line.matches(term).count();
            if occurrences > 0 {
                count += occurrences;
                *line = line.replace(term, replacement);
            }
        }
        if count > 0 {
            self.modified = true;
        }
        self.set_status(format!("Replaced {} occurrence(s)", count), false);
        count
    }

    // ── Scrolling ────────────────────────────────────────────────────────────

    pub fn update_scroll(&mut self, text_height: usize, text_width: usize) {
        // Vertical
        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if text_height > 0 && self.cursor_row >= self.scroll_row + text_height {
            self.scroll_row = self.cursor_row + 1 - text_height;
        }
        // Horizontal (char-based)
        let char_col = self.cursor_char_col();
        if char_col < self.scroll_col {
            self.scroll_col = char_col;
        } else if text_width > 0 && char_col >= self.scroll_col + text_width {
            self.scroll_col = char_col + 1 - text_width;
        }
    }
}

/// Returns the byte offsets of all non-overlapping occurrences of `term_lower`
/// in `line_lower`. Both arguments must already be lowercase.
fn find_matches_in_line(line_lower: &str, term_lower: &str) -> Vec<usize> {
    let term_len = term_lower.len();
    let mut matches = Vec::new();
    let mut start = 0usize;
    while start + term_len <= line_lower.len() {
        if line_lower[start..].starts_with(term_lower) {
            matches.push(start);
            start += term_len;
        } else {
            start += line_lower[start..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }
    }
    matches
}

/// Convert a char index to a byte offset in `s`, clamped to `s.len()`.
fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}
