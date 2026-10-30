use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use crate::remote_file::RemoteFile;
use crate::syntax;
use crossterm::{
    queue,
    style::{Color, ResetColor, SetForegroundColor},
};

pub struct Editor {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll: usize,
    pub file_path: PathBuf,
    pub modified: bool,
    pub last_op_was_cut: bool,
    pub undo_stack: Vec<(Vec<String>, usize, usize)>,
    pub message: Option<String>,
    pub clip_lines: Vec<String>,
    pub syntax: Option<syntax::Syntax>,
}

impl Editor {
    pub(crate) fn floor_char_boundary(s: &str, mut col: usize) -> usize {
        col = col.min(s.len());
        while col > 0 && !s.is_char_boundary(col) {
            col -= 1;
        }
        col
    }

    pub(crate) fn ceil_char_boundary(s: &str, mut col: usize) -> usize {
        col = col.min(s.len());
        while col < s.len() && !s.is_char_boundary(col) {
            col += 1;
        }
        col
    }

    pub(crate) fn clamp_col(s: &str, col: usize) -> usize {
        Self::floor_char_boundary(s, col)
    }

    pub(crate) fn next_char_boundary(s: &str, col: usize) -> usize {
        let col = Self::ceil_char_boundary(s, col);
        s[col..]
            .chars()
            .next()
            .map_or(s.len(), |c| col + c.len_utf8())
    }

    pub fn new(path: PathBuf) -> io::Result<Self> {
        let path_str = path.to_string_lossy();
        let content = Self::load(&path_str)?;
        // Keep the final empty line so saving preserves trailing newlines.
        let lines = content.split('\n').map(str::to_string).collect();
        let syntax = syntax::Syntax::load(path_str.as_ref());

        Ok(Self {
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll: 0,
            file_path: path,
            modified: false,
            last_op_was_cut: false,
            undo_stack: Vec::with_capacity(30),
            clip_lines: Vec::new(),
            message: Some("catbath".to_string()),
            syntax,
        })
    }

    pub fn render_line(&self, out: &mut impl Write, vis: &str) -> io::Result<()> {
        let syn = match &self.syntax {
            Some(s) => s,
            None => return out.write_all(vis.as_bytes()),
        };
        let bytes = vis.as_bytes();
        let comment = syn.comment.as_bytes();
        let mut quote_buffer = [0; 4];
        let quote = syn.string.encode_utf8(&mut quote_buffer).as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            // 1. Comments (lazy: just check prefix)
            if !comment.is_empty() && bytes[i..].starts_with(comment) {
                queue!(out, SetForegroundColor(Color::DarkGrey))?;
                out.write_all(&bytes[i..])?;
                queue!(out, ResetColor)?;
                return Ok(()); // Rest of line is comment
            }

            // 2. Strings, including escaped quotes and Unicode delimiters.
            if bytes[i..].starts_with(quote) {
                queue!(out, SetForegroundColor(Color::Green))?;
                out.write_all(quote)?;
                i += quote.len();
                let start = i;
                let mut escaped = false;
                for c in vis[start..].chars() {
                    i += c.len_utf8();
                    if escaped {
                        escaped = false;
                    } else if c == syn.string {
                        break;
                    } else if c == '\\' {
                        escaped = true;
                    }
                }
                out.write_all(&bytes[start..i])?;
                queue!(out, ResetColor)?;
                continue;
            }

            // 3. Keywords & Types (lazy: only alphanumeric)
            if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let word = &vis[start..i];
                if syn.keywords.contains(word) {
                    queue!(out, SetForegroundColor(Color::Yellow))?;
                } else if syn.types.contains(word) {
                    queue!(out, SetForegroundColor(Color::Cyan))?;
                }
                out.write_all(word.as_bytes())?;
                if syn.keywords.contains(word) || syn.types.contains(word) {
                    queue!(out, ResetColor)?;
                }
                continue;
            }

            // 4. Default text
            let c = vis[i..]
                .chars()
                .next()
                .expect("the byte offset is before the end of the line");
            out.write_all(&bytes[i..i + c.len_utf8()])?;
            i += c.len_utf8();
        }
        Ok(())
    }

    pub(crate) fn push_undo(&mut self) {
        self.last_op_was_cut = false;
        if self.undo_stack.len() >= 30 {
            self.undo_stack.remove(0);
        }
        self.undo_stack
            .push((self.lines.clone(), self.cursor_row, self.cursor_col));
    }

    pub fn undo(&mut self) {
        self.last_op_was_cut = false;
        if let Some((lines, row, col)) = self.undo_stack.pop() {
            self.lines = lines;
            self.cursor_row = row.min(self.lines.len().saturating_sub(1));
            self.cursor_col = self
                .lines
                .get(self.cursor_row)
                .map_or(0, |line| Self::clamp_col(line, col));
            self.modified = true;
            self.message = Some("undid last change".to_string());
        } else {
            self.message = Some("nothing to undo".to_string());
        }
    }

    pub fn load(path: &str) -> io::Result<String> {
        if let Some(remote) = RemoteFile::parse(path)? {
            return remote.load();
        }

        match fs::read_to_string(path) {
            Ok(content) => Ok(content),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error),
        }
    }

    pub fn save_to(path: &str, s: &str) -> io::Result<()> {
        if let Some(remote) = RemoteFile::parse(path)? {
            return remote.save(s);
        }
        fs::write(path, s)
    }

    pub fn save(&mut self) -> io::Result<()> {
        self.last_op_was_cut = false;
        let content = self.lines.join("\n");
        Self::save_to(&self.file_path.to_string_lossy(), &content)?;
        self.modified = false;
        self.message = Some(format!("> saved {}", self.file_path.display()));
        Ok(())
    }

    pub fn insert_char(&mut self, c: char) {
        self.push_undo();
        if self.cursor_row >= self.lines.len() {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.cursor_row];
        let col = Self::clamp_col(line, self.cursor_col);
        line.insert(col, c);
        self.cursor_col = col + c.len_utf8();
        self.modified = true;
        self.message = None;
    }

    pub fn insert_newline(&mut self) {
        self.push_undo();
        if self.cursor_row >= self.lines.len() {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.cursor_row];
        let col = Self::clamp_col(line, self.cursor_col);
        let rest = line.split_off(col);
        self.lines.insert(self.cursor_row + 1, rest);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.modified = true;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_col > 0 {
            self.push_undo();
            let line = &mut self.lines[self.cursor_row];
            let col = Self::ceil_char_boundary(line, self.cursor_col);
            if let Some((prev, _)) = line[..col].char_indices().last() {
                line.remove(prev);
                self.cursor_col = prev;
            } else {
                self.cursor_col = 0;
            }
            self.modified = true;
        } else if self.cursor_row > 0 {
            self.push_undo();
            let prev = self.cursor_row - 1;
            let curr = self.lines.remove(self.cursor_row);
            self.lines[prev].push_str(&curr);
            self.cursor_col = self.lines[prev].len() - curr.len();
            self.cursor_row = prev;
            self.modified = true;
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursor_row >= self.lines.len() {
            return;
        }
        let col = Self::clamp_col(&self.lines[self.cursor_row], self.cursor_col);
        self.cursor_col = col;
        if col < self.lines[self.cursor_row].len() {
            self.push_undo();
            self.lines[self.cursor_row].remove(col);
            self.modified = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.push_undo();
            let next = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next);
            self.modified = true;
        }
    }

    pub fn move_left(&mut self) {
        self.last_op_was_cut = false;
        if self.cursor_col > 0 && self.cursor_row < self.lines.len() {
            let line = &self.lines[self.cursor_row];
            let col = Self::clamp_col(line, self.cursor_col);
            self.cursor_col = line[..col].char_indices().last().map_or(0, |(i, _)| i);
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    pub fn move_right(&mut self) {
        self.last_op_was_cut = false;
        if self.cursor_row < self.lines.len() {
            let len = self.lines[self.cursor_row].len();
            let col = Self::ceil_char_boundary(&self.lines[self.cursor_row], self.cursor_col);
            if col < len {
                self.cursor_col = Self::next_char_boundary(&self.lines[self.cursor_row], col);
            } else if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
        }
    }

    pub fn move_up(&mut self) {
        self.last_op_was_cut = false;
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = Self::clamp_col(&self.lines[self.cursor_row], self.cursor_col);
        }
    }

    pub fn move_down(&mut self) {
        self.last_op_was_cut = false;
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = Self::clamp_col(&self.lines[self.cursor_row], self.cursor_col);
        }
    }

    pub fn move_home(&mut self) {
        self.last_op_was_cut = false;
        self.cursor_col = 0;
    }
    pub fn move_end(&mut self) {
        self.last_op_was_cut = false;
        if self.cursor_row < self.lines.len() {
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    pub fn page_up(&mut self, vis: usize) {
        self.last_op_was_cut = false;
        self.cursor_row = self.cursor_row.saturating_sub(vis.max(1));
        if self.cursor_row < self.lines.len() {
            self.cursor_col = Self::clamp_col(&self.lines[self.cursor_row], self.cursor_col);
        }
        self.scroll = self.scroll.saturating_sub(vis.max(1));
    }

    pub fn page_down(&mut self, vis: usize) {
        self.last_op_was_cut = false;
        self.cursor_row = (self.cursor_row + vis.max(1)).min(self.lines.len().saturating_sub(1));
        if self.cursor_row < self.lines.len() {
            self.cursor_col = Self::clamp_col(&self.lines[self.cursor_row], self.cursor_col);
        }
        self.scroll = (self.scroll + vis.max(1)).min(self.lines.len().saturating_sub(1));
    }

    pub fn adjust_scroll(&mut self, vis: usize) {
        if self.cursor_row < self.scroll {
            self.scroll = self.cursor_row;
        } else if self.cursor_row >= self.scroll + vis {
            self.scroll = self.cursor_row.saturating_sub(vis.saturating_sub(1));
        }
        let max_s = self.lines.len().saturating_sub(vis.max(1));
        if self.scroll > max_s {
            self.scroll = max_s;
        }
    }

    pub fn find_next(&mut self, query: &str) -> bool {
        self.last_op_was_cut = false;
        if query.is_empty() {
            return false;
        }
        let q: String = query.chars().flat_map(char::to_lowercase).collect();
        let start_r = self.cursor_row;
        let start_c = self
            .lines
            .get(start_r)
            .map_or(0, |line| Self::next_char_boundary(line, self.cursor_col));

        for r in start_r..self.lines.len() {
            let line = &self.lines[r];
            let from = if r == start_r { start_c } else { 0 };
            if let Some(p) = find_case_insensitive(&line[from..], &q) {
                self.cursor_row = r;
                self.cursor_col = from + p;
                self.message = Some(format!("found '{}'", query));
                return true;
            }
        }
        for r in 0..=start_r {
            let line = &self.lines[r];
            if let Some(p) = find_case_insensitive(line, &q) {
                self.cursor_row = r;
                self.cursor_col = p;
                self.message = Some(format!("found '{}' (wrapped)", query));
                return true;
            }
        }
        self.message = Some(format!("not found: '{}'", query));
        false
    }

    pub fn cut_line(&mut self) {
        if self.cursor_row >= self.lines.len() {
            return;
        }

        // If the last action wasn't a cut, clear the clipboard to start a new block.
        if !self.last_op_was_cut {
            self.clip_lines.clear();
        }
        self.push_undo();

        // Zero-clone cut: pull the String straight out of the Vec.
        let line = if self.lines.len() > 1 {
            self.lines.remove(self.cursor_row)
        } else {
            std::mem::take(&mut self.lines[0])
        };

        self.clip_lines.push(line);

        // Clamp cursor if we cut the last line
        if self.cursor_row >= self.lines.len() {
            self.cursor_row = self.lines.len() - 1;
        }

        self.cursor_col = 0;
        self.modified = true;
        self.last_op_was_cut = true; // We are in a cut sequence
    }

    pub fn paste(&mut self) {
        if self.clip_lines.is_empty() {
            return;
        }
        self.push_undo();

        // O(N) block insertion
        self.lines.splice(
            self.cursor_row..self.cursor_row,
            self.clip_lines.iter().cloned(),
        );

        self.cursor_row += self.clip_lines.len();
        self.cursor_col = 0;
        self.modified = true;
        self.last_op_was_cut = false; // Pasting breaks the cut sequence
    }
}

// Lowercasing can change UTF-8 lengths, so map the match back to the source.
fn find_case_insensitive(line: &str, query: &str) -> Option<usize> {
    let folded: String = line.chars().flat_map(char::to_lowercase).collect();
    let found = folded.find(query)?;
    let mut offset = 0;
    for (original, c) in line.char_indices() {
        offset += c.to_lowercase().map(char::len_utf8).sum::<usize>();
        if found < offset {
            return Some(original);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_directory::TestDirectory;

    fn editor_with(text: &str) -> Editor {
        Editor {
            lines: text.split('\n').map(String::from).collect(),
            cursor_row: 0,
            cursor_col: 0,
            scroll: 0,
            file_path: PathBuf::from("test.txt"),
            modified: false,
            last_op_was_cut: false,
            undo_stack: Vec::new(),
            message: None,
            clip_lines: Vec::new(),
            syntax: None,
        }
    }

    #[test]
    fn edits_stay_on_utf8_boundaries() {
        let mut ed = editor_with("");

        ed.insert_char('é');
        ed.insert_char('x');
        assert_eq!(ed.lines[0], "éx");

        ed.delete_char();
        assert_eq!(ed.lines[0], "é");

        ed.move_left();
        ed.delete_forward();
        assert_eq!(ed.lines[0], "");
    }

    #[test]
    fn search_after_utf8_does_not_repeat_current_match() {
        let mut ed = editor_with("éabc abc");
        ed.cursor_col = 2;

        assert!(ed.find_next("abc"));

        assert_eq!(ed.cursor_col, 6);
    }

    #[test]
    fn opening_and_saving_preserves_file_contents() {
        let dir = TestDirectory::new();
        let path = dir.path().join("roundtrip.txt");
        for content in ["", "hello", "hello\n", "hello\n\n", "a\r\nb\r\n"] {
            fs::write(&path, content).expect("write test input");
            let mut ed = Editor::new(path.clone()).expect("open test input");
            ed.save().expect("save test input");
            assert_eq!(fs::read_to_string(&path).expect("read saved file"), content);
        }
    }

    #[test]
    fn invalid_utf8_is_reported_instead_of_opening_an_empty_buffer() {
        let dir = TestDirectory::new();
        let path = dir.path().join("invalid.txt");
        fs::write(&path, [0xff]).expect("write invalid UTF-8");
        assert!(
            Editor::new(path).is_err(),
            "invalid input must not be silently discarded"
        );
    }

    #[test]
    fn missing_file_opens_as_an_empty_buffer() {
        let dir = TestDirectory::new();
        let ed = Editor::new(dir.path().join("new.txt")).expect("open a new file");
        assert_eq!(ed.lines, [""]);
    }

    #[test]
    fn search_wraps_to_an_earlier_match_on_the_current_line() {
        let mut ed = editor_with("abc tail");
        ed.move_end();
        assert!(ed.find_next("abc"), "search must wrap within a single line");
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn unicode_case_conversion_keeps_original_search_offsets() {
        let mut ed = editor_with("İ abc");
        assert!(
            ed.find_next("ABC"),
            "case-insensitive search must find ASCII after Unicode"
        );
        assert_eq!(ed.cursor_col, 3);

        let mut ed = editor_with("İ abc");
        ed.move_end();
        assert!(
            ed.find_next("ABC"),
            "wrapped search must use original byte offsets"
        );
        assert_eq!(ed.cursor_col, 3);
    }

    #[test]
    fn cut_can_be_undone() {
        let mut ed = editor_with("one\ntwo");
        ed.cut_line();
        ed.undo();
        assert_eq!(ed.lines, ["one", "two"]);
    }

    #[test]
    fn movement_breaks_a_sequence_of_cuts() {
        let mut ed = editor_with("one\ntwo\nthree");
        ed.cut_line();
        ed.move_down();
        ed.cut_line();
        assert_eq!(ed.clip_lines, ["three"]);
    }

    #[test]
    fn unicode_string_delimiters_do_not_split_utf8_with_color_codes() {
        let mut ed = editor_with("");
        ed.syntax = Some(syntax::Syntax {
            keywords: Default::default(),
            types: Default::default(),
            comment: "//".into(),
            string: 'ƀ',
        });
        let mut output = Vec::new();
        ed.render_line(&mut output, "Ā ƀtextƀ")
            .expect("render Unicode syntax");
        let rendered =
            String::from_utf8(output).expect("color codes must occur between complete characters");
        assert!(rendered.contains("ƀtextƀ"), "keep a quoted string together");
    }

    #[test]
    fn escaped_quotes_do_not_end_string_highlighting() {
        let mut ed = editor_with("");
        ed.syntax = Some(syntax::Syntax {
            keywords: Default::default(),
            types: Default::default(),
            comment: "//".into(),
            string: '"',
        });
        let mut output = Vec::new();
        ed.render_line(&mut output, r#""a\" // still a string" // comment"#)
            .expect("render an escaped quote");
        let rendered = String::from_utf8(output).expect("UTF-8 output");
        assert!(
            rendered.contains(r#""a\" // still a string""#),
            "escaped quotes must stay inside the highlighted string"
        );
    }
}
