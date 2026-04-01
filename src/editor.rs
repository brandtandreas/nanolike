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
    pub clipboard: String,
    pub selection_anchor: Option<(usize, usize)>,
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
            clipboard: String::new(),
            selection_anchor: None,
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

    // ── Selection ────────────────────────────────────────────────────────────

    pub fn has_selection(&self) -> bool {
        match self.selection_anchor {
            Some((ar, ac)) => (ar, ac) != (self.cursor_row, self.cursor_col),
            None => false,
        }
    }

    /// Returns `Some((start, end))` in document order if a non-empty selection
    /// exists, where each end is `(row, byte_col)`.
    pub fn selection_ordered(&self) -> Option<((usize, usize), (usize, usize))> {
        let (ar, ac) = self.selection_anchor?;
        let (cr, cc) = (self.cursor_row, self.cursor_col);
        if (ar, ac) == (cr, cc) {
            return None;
        }
        if (ar, ac) <= (cr, cc) {
            Some(((ar, ac), (cr, cc)))
        } else {
            Some(((cr, cc), (ar, ac)))
        }
    }

    pub fn get_selected_text(&self) -> String {
        let Some(((sr, sc), (er, ec))) = self.selection_ordered() else {
            return String::new();
        };
        if sr == er {
            return self.lines[sr][sc..ec].to_string();
        }
        let mut out = self.lines[sr][sc..].to_string();
        for row in sr + 1..er {
            out.push('\n');
            out.push_str(&self.lines[row]);
        }
        out.push('\n');
        out.push_str(&self.lines[er][..ec]);
        out
    }

    /// Delete the selected region, move cursor to the selection start, and
    /// clear the anchor. Saves an undo snapshot before modifying.
    pub fn delete_selection(&mut self) {
        let Some(((sr, sc), (er, ec))) = self.selection_ordered() else {
            return;
        };
        self.save_undo();
        if sr == er {
            self.lines[sr].drain(sc..ec);
        } else {
            let end_tail = self.lines[er][ec..].to_string();
            self.lines[sr].truncate(sc);
            self.lines[sr].push_str(&end_tail);
            self.lines.drain(sr + 1..=er);
        }
        self.cursor_row = sr;
        self.cursor_col = sc;
        self.selection_anchor = None;
        self.modified = true;
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some((0, 0));
        self.cursor_row = self.lines.len() - 1;
        self.cursor_col = self.lines.last().map(|l| l.len()).unwrap_or(0);
        self.set_status("All selected", false);
    }

    // ── Editing ──────────────────────────────────────────────────────────────

    pub fn insert_char(&mut self, ch: char) {
        if self.has_selection() {
            self.delete_selection();
        }
        self.save_undo();
        let col = self.cursor_col;
        self.lines[self.cursor_row].insert(col, ch);
        self.cursor_col += ch.len_utf8();
        self.modified = true;
    }

    pub fn insert_str(&mut self, s: &str) {
        if self.has_selection() {
            self.delete_selection();
        }
        self.save_undo();
        let col = self.cursor_col;
        self.lines[self.cursor_row].insert_str(col, s);
        self.cursor_col += s.len();
        self.modified = true;
    }

    pub fn insert_newline(&mut self, auto_indent: bool) {
        if self.has_selection() {
            self.delete_selection();
        }
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
        if self.has_selection() {
            self.delete_selection();
            return;
        }
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
        if self.has_selection() {
            self.delete_selection();
            return;
        }
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
        if self.has_selection() {
            self.clipboard = self.get_selected_text();
            self.delete_selection();
            self.set_status("Selection cut", false);
            return;
        }
        self.save_undo();
        let line = self.lines.remove(self.cursor_row);
        self.clipboard = line + "\n";
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.clamp_cursor();
        self.modified = true;
        self.set_status("Line cut", false);
    }

    pub fn copy_line(&mut self) {
        if self.has_selection() {
            self.clipboard = self.get_selected_text();
            self.selection_anchor = None;
            self.set_status("Selection copied", false);
            return;
        }
        self.clipboard = self.lines[self.cursor_row].clone() + "\n";
        self.set_status("Line copied", false);
    }

    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            self.set_status("Clipboard empty", false);
            return;
        }
        if self.has_selection() {
            self.delete_selection();
        }
        self.save_undo();
        let text = self.clipboard.clone();
        let parts: Vec<&str> = text.split('\n').collect();
        if parts.len() == 1 {
            // Inline paste (no newline in clipboard)
            let col = self.cursor_col;
            self.lines[self.cursor_row].insert_str(col, parts[0]);
            self.cursor_col += parts[0].len();
        } else {
            // Multi-line paste
            let col = self.cursor_col;
            let tail = self.lines[self.cursor_row][col..].to_string();
            self.lines[self.cursor_row].truncate(col);
            self.lines[self.cursor_row].push_str(parts[0]);
            let insert_row = self.cursor_row + 1;
            for (i, &chunk) in parts[1..].iter().enumerate() {
                self.lines.insert(insert_row + i, chunk.to_string());
            }
            // Append the original tail to the last inserted line
            let last_row = insert_row + parts.len() - 2;
            let last_chunk_len = parts.last().map(|s| s.len()).unwrap_or(0);
            self.lines[last_row].push_str(&tail);
            self.cursor_row = last_row;
            self.cursor_col = last_chunk_len;
        }
        self.modified = true;
        self.set_status("Pasted", false);
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Build an Editor whose content is `text` (lines separated by `\n`).
    /// Cursor starts at (0, 0).
    fn make_editor(text: &str) -> Editor {
        let mut ed = Editor::new(None);
        ed.lines = text.lines().map(|l| l.to_string()).collect();
        if ed.lines.is_empty() {
            ed.lines.push(String::new());
        }
        ed.saved_lines = ed.lines.clone();
        ed.modified = false;
        ed
    }

    // ── find_matches_in_line ─────────────────────────────────────────────────
    // Note: find_matches_in_line is never called with an empty term in practice
    // (build_search_matches guards against it); no empty-term test here.

    #[test]
    fn fml_no_match() {
        assert_eq!(find_matches_in_line("hello world", "xyz"), Vec::<usize>::new());
    }

    #[test]
    fn fml_single_match_at_start() {
        assert_eq!(find_matches_in_line("hello world", "hello"), vec![0]);
    }

    #[test]
    fn fml_single_match_at_end() {
        assert_eq!(find_matches_in_line("say hello", "hello"), vec![4]);
    }

    #[test]
    fn fml_multiple_matches() {
        assert_eq!(find_matches_in_line("aaa", "a"), vec![0, 1, 2]);
    }

    #[test]
    fn fml_non_overlapping() {
        // "aa" in "aaaa" should give offsets 0 and 2 (not 0, 1, 2)
        assert_eq!(find_matches_in_line("aaaa", "aa"), vec![0, 2]);
    }

    #[test]
    fn fml_multibyte_chars() {
        // "é" is 2 bytes; "héllo" has 'h'=0, 'é'=1..3, 'l'=3, 'l'=4, 'o'=5
        let line = "héllo héllo";
        let term = "héllo";
        let matches = find_matches_in_line(line, term);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], 0);
    }

    // ── char_to_byte ─────────────────────────────────────────────────────────

    #[test]
    fn ctb_ascii_positions() {
        assert_eq!(char_to_byte("hello", 0), 0);
        assert_eq!(char_to_byte("hello", 2), 2);
        assert_eq!(char_to_byte("hello", 4), 4);
    }

    #[test]
    fn ctb_multibyte() {
        // "héllo": h=0, é=1 (2 bytes), l=3, l=4, o=5
        assert_eq!(char_to_byte("héllo", 0), 0);
        assert_eq!(char_to_byte("héllo", 1), 1); // start of 'é'
        assert_eq!(char_to_byte("héllo", 2), 3); // 'l' after 'é'
    }

    #[test]
    fn ctb_beyond_length_clamped() {
        assert_eq!(char_to_byte("hi", 100), 2);
    }

    #[test]
    fn ctb_empty_string() {
        assert_eq!(char_to_byte("", 0), 0);
        assert_eq!(char_to_byte("", 5), 0);
    }

    // ── UndoStack ────────────────────────────────────────────────────────────

    fn snap(lines: Vec<&str>, row: usize, col: usize) -> Snapshot {
        Snapshot {
            lines: lines.into_iter().map(|s| s.to_string()).collect(),
            cursor_row: row,
            cursor_col: col,
        }
    }

    #[test]
    fn undostack_new_is_empty() {
        let us = UndoStack::new();
        assert!(!us._can_undo());
        assert!(!us._can_redo());
    }

    #[test]
    fn undostack_push_enables_undo() {
        let mut us = UndoStack::new();
        us.push_edit(snap(vec!["hello"], 0, 0));
        assert!(us._can_undo());
        assert!(!us._can_redo());
    }

    #[test]
    fn undostack_do_undo_returns_snapshot() {
        let mut us = UndoStack::new();
        let s = snap(vec!["original"], 0, 0);
        us.push_edit(s.clone());
        let restored = us.do_undo(snap(vec!["changed"], 0, 0)).unwrap();
        assert_eq!(restored.lines, vec!["original"]);
        assert!(us._can_redo());
        assert!(!us._can_undo());
    }

    #[test]
    fn undostack_do_redo_returns_snapshot() {
        let mut us = UndoStack::new();
        us.push_edit(snap(vec!["original"], 0, 0));
        let current = snap(vec!["changed"], 0, 0);
        let _prev = us.do_undo(current).unwrap();
        let redone = us.do_redo(snap(vec!["original"], 0, 0)).unwrap();
        assert_eq!(redone.lines, vec!["changed"]);
    }

    #[test]
    fn undostack_undo_empty_returns_none() {
        let mut us = UndoStack::new();
        assert!(us.do_undo(snap(vec!["x"], 0, 0)).is_none());
    }

    #[test]
    fn undostack_redo_empty_returns_none() {
        let mut us = UndoStack::new();
        assert!(us.do_redo(snap(vec!["x"], 0, 0)).is_none());
    }

    #[test]
    fn undostack_push_clears_redo() {
        let mut us = UndoStack::new();
        us.push_edit(snap(vec!["a"], 0, 0));
        us.do_undo(snap(vec!["b"], 0, 0)).unwrap();
        assert!(us._can_redo());
        // Push a new edit — redo should be cleared
        us.push_edit(snap(vec!["c"], 0, 0));
        assert!(!us._can_redo());
    }

    #[test]
    fn undostack_max_undo_evicts_oldest() {
        let mut us = UndoStack::new();
        for i in 0..=MAX_UNDO {
            us.push_edit(snap(vec![&format!("line {}", i)], 0, 0));
        }
        // Should still have MAX_UNDO entries (oldest dropped)
        let mut count = 0;
        let mut current = snap(vec!["current"], 0, 0);
        while let Some(prev) = us.do_undo(current.clone()) {
            count += 1;
            current = prev;
        }
        assert_eq!(count, MAX_UNDO);
    }

    // ── Editor::new ──────────────────────────────────────────────────────────

    #[test]
    fn editor_new_has_one_empty_line() {
        let ed = Editor::new(None);
        assert_eq!(ed.lines, vec![""]);
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 0);
        assert!(!ed.modified);
    }

    // ── Editing ───────────────────────────────────────────────────────────────

    #[test]
    fn insert_char_ascii() {
        let mut ed = make_editor("hello");
        ed.cursor_col = 5;
        ed.insert_char('!');
        assert_eq!(ed.lines[0], "hello!");
        assert_eq!(ed.cursor_col, 6);
        assert!(ed.modified);
    }

    #[test]
    fn insert_char_multibyte() {
        let mut ed = make_editor("");
        ed.insert_char('é');
        assert_eq!(ed.lines[0], "é");
        assert_eq!(ed.cursor_col, 2); // 'é' is 2 bytes
    }

    #[test]
    fn insert_str_basic() {
        let mut ed = make_editor("");
        ed.insert_str("hello");
        assert_eq!(ed.lines[0], "hello");
        assert_eq!(ed.cursor_col, 5);
    }

    #[test]
    fn insert_newline_no_indent() {
        let mut ed = make_editor("hello world");
        ed.cursor_col = 5;
        ed.insert_newline(false);
        assert_eq!(ed.lines.len(), 2);
        assert_eq!(ed.lines[0], "hello");
        assert_eq!(ed.lines[1], " world");
        assert_eq!(ed.cursor_row, 1);
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn insert_newline_with_auto_indent() {
        let mut ed = make_editor("    hello");
        ed.cursor_col = 9; // end of line
        ed.insert_newline(true);
        assert_eq!(ed.cursor_col, 4); // 4 spaces indent
        assert_eq!(&ed.lines[1][..4], "    ");
    }

    #[test]
    fn backspace_removes_char() {
        let mut ed = make_editor("hello");
        ed.cursor_col = 5;
        ed.backspace();
        assert_eq!(ed.lines[0], "hell");
        assert_eq!(ed.cursor_col, 4);
    }

    #[test]
    fn backspace_at_line_start_merges_lines() {
        let mut ed = make_editor("hello\nworld");
        ed.cursor_row = 1;
        ed.cursor_col = 0;
        ed.backspace();
        assert_eq!(ed.lines.len(), 1);
        assert_eq!(ed.lines[0], "helloworld");
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 5);
    }

    #[test]
    fn backspace_multibyte_char() {
        let mut ed = make_editor("hé");
        ed.cursor_col = 3; // after 'é' (2 bytes) + 'h' (1 byte)
        ed.backspace();
        assert_eq!(ed.lines[0], "h");
        assert_eq!(ed.cursor_col, 1);
    }

    #[test]
    fn delete_char_removes_forward() {
        let mut ed = make_editor("hello");
        ed.cursor_col = 0;
        ed.delete_char();
        assert_eq!(ed.lines[0], "ello");
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn delete_char_at_line_end_merges() {
        let mut ed = make_editor("hello\nworld");
        ed.cursor_col = 5; // end of first line
        ed.delete_char();
        assert_eq!(ed.lines.len(), 1);
        assert_eq!(ed.lines[0], "helloworld");
    }

    #[test]
    fn delete_to_eol_truncates() {
        let mut ed = make_editor("hello world");
        ed.cursor_col = 5;
        ed.delete_to_eol();
        assert_eq!(ed.lines[0], "hello");
        assert!(ed.modified);
    }

    #[test]
    fn delete_word_before_removes_word() {
        let mut ed = make_editor("hello world");
        ed.cursor_col = 11; // end of "world"
        ed.delete_word_before();
        assert_eq!(ed.lines[0], "hello ");
    }

    #[test]
    fn delete_word_before_at_col_zero_noop() {
        let mut ed = make_editor("hello");
        ed.cursor_col = 0;
        ed.delete_word_before();
        assert_eq!(ed.lines[0], "hello");
    }

    // ── Navigation ───────────────────────────────────────────────────────────

    #[test]
    fn move_left_basic() {
        let mut ed = make_editor("hello");
        ed.cursor_col = 3;
        ed.move_left();
        assert_eq!(ed.cursor_col, 2);
    }

    #[test]
    fn move_left_wraps_to_previous_line() {
        let mut ed = make_editor("hello\nworld");
        ed.cursor_row = 1;
        ed.cursor_col = 0;
        ed.move_left();
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 5); // end of "hello"
    }

    #[test]
    fn move_right_basic() {
        let mut ed = make_editor("hello");
        ed.cursor_col = 2;
        ed.move_right();
        assert_eq!(ed.cursor_col, 3);
    }

    #[test]
    fn move_right_wraps_to_next_line() {
        let mut ed = make_editor("hello\nworld");
        ed.cursor_col = 5; // end of "hello"
        ed.move_right();
        assert_eq!(ed.cursor_row, 1);
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn move_up_preserves_char_col() {
        let mut ed = make_editor("hello\nhi");
        ed.cursor_row = 1;
        ed.cursor_col = 2; // char col 2, but "hi" is only 2 chars so col=2
        ed.move_up();
        assert_eq!(ed.cursor_row, 0);
        // char col 2 in "hello" → byte col 2
        assert_eq!(ed.cursor_col, 2);
    }

    #[test]
    fn move_down_clamps_to_line_end() {
        let mut ed = make_editor("hello world\nhi");
        ed.cursor_col = 11; // end of first line
        ed.move_down();
        assert_eq!(ed.cursor_row, 1);
        // "hi" has only 2 chars so cursor clamps at byte 2
        assert_eq!(ed.cursor_col, 2);
    }

    #[test]
    fn move_home_smart_first_press() {
        let mut ed = make_editor("    hello");
        ed.cursor_col = 9; // end of line
        ed.move_home();
        assert_eq!(ed.cursor_col, 4); // indent = 4 spaces
    }

    #[test]
    fn move_home_smart_second_press() {
        let mut ed = make_editor("    hello");
        ed.cursor_col = 4; // already at indent
        ed.move_home();
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn move_end_goes_to_line_end() {
        let mut ed = make_editor("hello");
        ed.move_end();
        assert_eq!(ed.cursor_col, 5);
    }

    #[test]
    fn move_next_word_basic() {
        let mut ed = make_editor("hello world");
        ed.cursor_col = 0;
        ed.move_next_word();
        // "hello" is 5 chars; next word starts at 6
        assert_eq!(ed.cursor_col, 6);
    }

    #[test]
    fn move_prev_word_basic() {
        let mut ed = make_editor("hello world");
        ed.cursor_col = 11; // end
        ed.move_prev_word();
        assert_eq!(ed.cursor_col, 6); // start of "world"
    }

    #[test]
    fn goto_line_basic() {
        let mut ed = make_editor("a\nb\nc");
        ed.goto_line(2);
        assert_eq!(ed.cursor_row, 2);
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn goto_line_clamped() {
        let mut ed = make_editor("a\nb\nc");
        ed.goto_line(100);
        assert_eq!(ed.cursor_row, 2); // last line
    }

    #[test]
    fn move_page_down_clamped() {
        let mut ed = make_editor("a\nb\nc");
        ed.cursor_row = 0;
        ed.move_page_down(100);
        assert_eq!(ed.cursor_row, 2);
    }

    #[test]
    fn move_page_up_clamped() {
        let mut ed = make_editor("a\nb\nc");
        ed.cursor_row = 2;
        ed.move_page_up(100);
        assert_eq!(ed.cursor_row, 0);
    }

    #[test]
    fn clamp_cursor_fixes_row() {
        let mut ed = make_editor("hello");
        ed.cursor_row = 100;
        ed.clamp_cursor();
        assert_eq!(ed.cursor_row, 0);
    }

    #[test]
    fn clamp_cursor_fixes_col() {
        let mut ed = make_editor("hi");
        ed.cursor_col = 100;
        ed.clamp_cursor();
        assert_eq!(ed.cursor_col, 2);
    }

    // ── Selection ─────────────────────────────────────────────────────────────

    #[test]
    fn has_selection_false_without_anchor() {
        let ed = make_editor("hello");
        assert!(!ed.has_selection());
    }

    #[test]
    fn has_selection_false_when_anchor_equals_cursor() {
        let mut ed = make_editor("hello");
        ed.selection_anchor = Some((0, 0));
        assert!(!ed.has_selection());
    }

    #[test]
    fn has_selection_true_when_different() {
        let mut ed = make_editor("hello");
        ed.selection_anchor = Some((0, 0));
        ed.cursor_col = 3;
        assert!(ed.has_selection());
    }

    #[test]
    fn selection_ordered_forward() {
        let mut ed = make_editor("hello");
        ed.selection_anchor = Some((0, 1));
        ed.cursor_col = 4;
        let result = ed.selection_ordered().unwrap();
        assert_eq!(result, ((0, 1), (0, 4)));
    }

    #[test]
    fn selection_ordered_backward() {
        let mut ed = make_editor("hello");
        ed.selection_anchor = Some((0, 4));
        ed.cursor_col = 1;
        let result = ed.selection_ordered().unwrap();
        assert_eq!(result, ((0, 1), (0, 4)));
    }

    #[test]
    fn selection_ordered_none_when_equal() {
        let mut ed = make_editor("hello");
        ed.selection_anchor = Some((0, 2));
        ed.cursor_col = 2;
        assert!(ed.selection_ordered().is_none());
    }

    #[test]
    fn get_selected_text_single_line() {
        let mut ed = make_editor("hello world");
        ed.selection_anchor = Some((0, 6));
        ed.cursor_col = 11;
        assert_eq!(ed.get_selected_text(), "world");
    }

    #[test]
    fn get_selected_text_multiline() {
        let mut ed = make_editor("hello\nworld");
        ed.selection_anchor = Some((0, 3));
        ed.cursor_row = 1;
        ed.cursor_col = 3;
        let text = ed.get_selected_text();
        assert_eq!(text, "lo\nwor");
    }

    #[test]
    fn select_all() {
        let mut ed = make_editor("hello\nworld");
        ed.select_all();
        assert_eq!(ed.selection_anchor, Some((0, 0)));
        assert_eq!(ed.cursor_row, 1);
        assert_eq!(ed.cursor_col, 5);
    }

    #[test]
    fn delete_selection_single_line() {
        let mut ed = make_editor("hello world");
        ed.selection_anchor = Some((0, 5));
        ed.cursor_col = 11;
        ed.delete_selection();
        assert_eq!(ed.lines[0], "hello");
        assert_eq!(ed.cursor_col, 5);
        assert!(ed.selection_anchor.is_none());
    }

    #[test]
    fn delete_selection_multiline() {
        let mut ed = make_editor("hello\nworld");
        ed.selection_anchor = Some((0, 2));
        ed.cursor_row = 1;
        ed.cursor_col = 3;
        ed.delete_selection();
        assert_eq!(ed.lines.len(), 1);
        assert_eq!(ed.lines[0], "held");
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 2);
    }

    // ── Clipboard ─────────────────────────────────────────────────────────────

    #[test]
    fn copy_line_stores_content() {
        let mut ed = make_editor("hello");
        ed.copy_line();
        assert_eq!(ed.clipboard, "hello\n");
        assert!(!ed.modified);
        assert_eq!(ed.lines[0], "hello"); // unchanged
    }

    #[test]
    fn copy_line_with_selection() {
        let mut ed = make_editor("hello world");
        ed.selection_anchor = Some((0, 6));
        ed.cursor_col = 11;
        ed.copy_line();
        assert_eq!(ed.clipboard, "world");
    }

    #[test]
    fn cut_line_removes_and_stores() {
        let mut ed = make_editor("hello\nworld");
        ed.cut_line();
        assert_eq!(ed.clipboard, "hello\n");
        assert_eq!(ed.lines, vec!["world"]);
        assert!(ed.modified);
    }

    #[test]
    fn cut_line_single_line_leaves_empty() {
        let mut ed = make_editor("hello");
        ed.cut_line();
        assert_eq!(ed.lines, vec![""]);
    }

    #[test]
    fn paste_empty_clipboard_noop() {
        let mut ed = make_editor("hello");
        ed.clipboard = String::new();
        ed.paste();
        assert_eq!(ed.lines[0], "hello");
        assert!(!ed.modified);
    }

    #[test]
    fn paste_inline() {
        let mut ed = make_editor("helo");
        ed.cursor_col = 3;
        ed.clipboard = "l".to_string();
        ed.paste();
        assert_eq!(ed.lines[0], "hello");
        assert_eq!(ed.cursor_col, 4);
    }

    #[test]
    fn paste_multiline() {
        let mut ed = make_editor("ac");
        ed.cursor_col = 1;
        ed.clipboard = "b\n".to_string(); // "b\n" splits into ["b", ""]
        ed.paste();
        assert_eq!(ed.lines, vec!["ab", "c"]);
    }

    // ── Undo / Redo ───────────────────────────────────────────────────────────

    #[test]
    fn undo_reverts_insert() {
        let mut ed = make_editor("hello");
        ed.cursor_col = 5;
        ed.insert_char('!');
        assert_eq!(ed.lines[0], "hello!");
        ed.do_undo();
        assert_eq!(ed.lines[0], "hello");
    }

    #[test]
    fn undo_nothing_sets_status() {
        let mut ed = make_editor("hello");
        ed.do_undo();
        assert_eq!(ed.status_msg, "Nothing to undo");
    }

    #[test]
    fn redo_reapplies_change() {
        let mut ed = make_editor("hello");
        ed.cursor_col = 5;
        ed.insert_char('!');
        ed.do_undo();
        ed.do_redo();
        assert_eq!(ed.lines[0], "hello!");
    }

    #[test]
    fn redo_nothing_sets_status() {
        let mut ed = make_editor("hello");
        ed.do_redo();
        assert_eq!(ed.status_msg, "Nothing to redo");
    }

    #[test]
    fn multiple_undo_steps() {
        let mut ed = make_editor("");
        ed.insert_char('a');
        ed.insert_char('b');
        ed.insert_char('c');
        ed.do_undo();
        assert_eq!(ed.lines[0], "ab");
        ed.do_undo();
        assert_eq!(ed.lines[0], "a");
        ed.do_undo();
        assert_eq!(ed.lines[0], "");
    }

    // ── Search ────────────────────────────────────────────────────────────────

    #[test]
    fn build_search_matches_empty_term() {
        let mut ed = make_editor("hello world");
        ed.search_term = String::new();
        ed.build_search_matches();
        assert!(ed.search_matches.is_empty());
    }

    #[test]
    fn build_search_matches_finds_matches() {
        let mut ed = make_editor("hello world hello");
        ed.search_term = "hello".to_string();
        ed.build_search_matches();
        assert_eq!(ed.search_matches.len(), 2);
        assert_eq!(ed.search_matches[0], (0, 0));
        assert_eq!(ed.search_matches[1], (0, 12));
    }

    #[test]
    fn build_search_matches_case_insensitive() {
        let mut ed = make_editor("Hello HELLO hello");
        ed.search_term = "hello".to_string();
        ed.build_search_matches();
        assert_eq!(ed.search_matches.len(), 3);
    }

    #[test]
    fn search_next_moves_cursor() {
        let mut ed = make_editor("hello world");
        ed.search_term = "world".to_string();
        ed.build_search_matches();
        ed.cursor_col = 0;
        let found = ed.search_next();
        assert!(found);
        assert_eq!(ed.cursor_col, 6);
    }

    #[test]
    fn search_next_wraps_around() {
        let mut ed = make_editor("hello world");
        ed.search_term = "hello".to_string();
        ed.build_search_matches();
        ed.cursor_col = 5; // past "hello"
        ed.search_next(); // wraps to the only match
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn search_next_no_matches_returns_false() {
        let mut ed = make_editor("hello world");
        ed.search_term = "xyz".to_string();
        ed.build_search_matches();
        assert!(!ed.search_next());
    }

    #[test]
    fn search_prev_moves_cursor_backward() {
        let mut ed = make_editor("hello hello");
        ed.search_term = "hello".to_string();
        ed.build_search_matches();
        ed.cursor_col = 10; // at second match area
        ed.search_prev();
        assert_eq!(ed.cursor_col, 6);
    }

    #[test]
    fn search_prev_wraps_around() {
        let mut ed = make_editor("hello hello");
        ed.search_term = "hello".to_string();
        ed.build_search_matches();
        ed.cursor_col = 0;
        ed.search_prev(); // wraps to last match
        assert_eq!(ed.cursor_col, 6);
    }

    #[test]
    fn replace_all_empty_term_returns_zero() {
        let mut ed = make_editor("hello world");
        let count = ed.replace_all("", "replacement");
        assert_eq!(count, 0);
        assert!(!ed.modified);
    }

    #[test]
    fn replace_all_replaces_and_returns_count() {
        let mut ed = make_editor("hello hello world");
        let count = ed.replace_all("hello", "bye");
        assert_eq!(count, 2);
        assert_eq!(ed.lines[0], "bye bye world");
        assert!(ed.modified);
    }

    #[test]
    fn replace_all_no_match_not_modified() {
        let mut ed = make_editor("hello world");
        ed.modified = false;
        let count = ed.replace_all("xyz", "abc");
        assert_eq!(count, 0);
        assert!(!ed.modified);
    }

    // ── Scrolling ─────────────────────────────────────────────────────────────

    #[test]
    fn update_scroll_scrolls_down() {
        let mut ed = make_editor("a\nb\nc\nd\ne");
        ed.cursor_row = 4;
        ed.scroll_row = 0;
        ed.update_scroll(3, 80);
        assert_eq!(ed.scroll_row, 2); // cursor_row(4) + 1 - height(3)
    }

    #[test]
    fn update_scroll_scrolls_up() {
        let mut ed = make_editor("a\nb\nc\nd\ne");
        ed.cursor_row = 1;
        ed.scroll_row = 3;
        ed.update_scroll(3, 80);
        assert_eq!(ed.scroll_row, 1);
    }

    #[test]
    fn update_scroll_horizontal() {
        let mut ed = make_editor("hello world this is a long line");
        ed.cursor_col = 30;
        ed.scroll_col = 0;
        ed.update_scroll(20, 10);
        // char_col = 30, scroll_col should be 30 + 1 - 10 = 21
        assert_eq!(ed.scroll_col, 21);
    }
}
