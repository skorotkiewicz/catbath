use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

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
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let path_str = path.to_string_lossy();
        let content = Self::load(&path_str).unwrap_or_default();
        let lines = if content.is_empty() {
            vec!["".to_string()]
        } else {
            content.lines().map(str::to_string).collect()
        };
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
        let mut i = 0;

        while i < bytes.len() {
            // 1. Comments (lazy: just check prefix)
            if !comment.is_empty() && bytes[i..].starts_with(comment) {
                queue!(out, SetForegroundColor(Color::DarkGrey))?;
                out.write_all(&bytes[i..])?;
                queue!(out, ResetColor)?;
                return Ok(()); // Rest of line is comment
            }

            // 2. Strings (lazy: no escape logic, just find next quote)
            if bytes[i] == syn.string as u8 {
                queue!(out, SetForegroundColor(Color::Green))?;
                out.write_all(&[bytes[i]])?;
                i += 1;
                while i < bytes.len() && bytes[i] != syn.string as u8 {
                    out.write_all(&[bytes[i]])?;
                    i += 1;
                }
                if i < bytes.len() {
                    out.write_all(&[bytes[i]])?; // closing quote
                    i += 1;
                }
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
                } else {
                    queue!(out, ResetColor)?;
                }
                out.write_all(word.as_bytes())?;
                queue!(out, ResetColor)?;
                continue;
            }

            // 4. Default text
            out.write_all(&[bytes[i]])?;
            i += 1;
        }
        Ok(())
    }

    pub(crate) fn push_undo(&mut self) {
        if self.undo_stack.len() >= 30 {
            self.undo_stack.remove(0);
        }
        self.undo_stack
            .push((self.lines.clone(), self.cursor_row, self.cursor_col));
    }

    pub fn undo(&mut self) {
        if let Some((lines, row, col)) = self.undo_stack.pop() {
            self.lines = lines;
            self.cursor_row = row.min(self.lines.len().saturating_sub(1));
            let llen = self.lines.get(self.cursor_row).map_or(0, |l| l.len());
            self.cursor_col = col.min(llen);
            self.modified = true;
            self.message = Some("undid last change".to_string());
        } else {
            self.message = Some("nothing to undo".to_string());
        }
    }

    pub fn load(path: &str) -> io::Result<String> {
        if let Some(rest) = path.strip_prefix("ssh://") {
            // Split "user@host" from "/path/to/file"
            let (target, remote_path) = rest.split_once('/').unwrap_or((rest, ""));
            let remote_path = format!("/{}", remote_path);

            // Executes: ssh user@host cat /path/to/file
            let out = Command::new("ssh")
                .arg(target)
                .arg("cat")
                .arg(&remote_path)
                .output()?;

            if !out.status.success() {
                return Err(io::Error::other(
                    String::from_utf8_lossy(&out.stderr).to_string(),
                ));
            }
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }

        if Path::new(path).exists() {
            fs::read_to_string(path)
        } else {
            Ok(String::new())
        }
    }

    pub fn save_to(path: &str, s: &str) -> io::Result<()> {
        if let Some(rest) = path.strip_prefix("ssh://") {
            let (target, remote_path) = rest.split_once('/').unwrap_or((rest, ""));
            let remote_path = format!("/{}", remote_path);
            let mut child = Command::new("ssh")
                .arg(target)
                .arg("tee")
                .arg(&remote_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                stdin.write_all(s.as_bytes())?;
            }
            if !child.wait()?.success() {
                return Err(io::Error::other("ssh save failed"));
            }
            return Ok(());
        }
        fs::write(path, s)
    }

    pub fn save(&mut self) -> io::Result<()> {
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
        let col = self.cursor_col.min(line.len());
        line.insert(col, c);
        self.cursor_col = col + 1;
        self.modified = true;
        self.message = None;
    }

    pub fn insert_newline(&mut self) {
        self.push_undo();
        if self.cursor_row >= self.lines.len() {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.cursor_row];
        let col = self.cursor_col.min(line.len());
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
            let col = self.cursor_col.min(line.len());
            line.remove(col - 1);
            self.cursor_col = col - 1;
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
        if self.cursor_col < self.lines[self.cursor_row].len() {
            self.push_undo();
            self.lines[self.cursor_row].remove(self.cursor_col);
            self.modified = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.push_undo();
            let next = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next);
            self.modified = true;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_row < self.lines.len() {
            let len = self.lines[self.cursor_row].len();
            if self.cursor_col < len {
                self.cursor_col += 1;
            } else if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            let len = self.lines[self.cursor_row].len();
            self.cursor_col = self.cursor_col.min(len);
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            let len = self.lines[self.cursor_row].len();
            self.cursor_col = self.cursor_col.min(len);
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }
    pub fn move_end(&mut self) {
        if self.cursor_row < self.lines.len() {
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    pub fn page_up(&mut self, vis: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(vis.max(1));
        if self.cursor_row < self.lines.len() {
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
        }
        self.scroll = self.scroll.saturating_sub(vis.max(1));
    }

    pub fn page_down(&mut self, vis: usize) {
        self.cursor_row = (self.cursor_row + vis.max(1)).min(self.lines.len().saturating_sub(1));
        if self.cursor_row < self.lines.len() {
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
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
        if query.is_empty() {
            return false;
        }
        let q = query.to_lowercase();
        let start_r = self.cursor_row;
        let start_c = self.cursor_col + 1;

        for r in start_r..self.lines.len() {
            let line = &self.lines[r];
            let from = if r == start_r { start_c } else { 0 };
            if let Some(p) = line.get(from..).unwrap_or("").to_lowercase().find(&q) {
                self.cursor_row = r;
                self.cursor_col = from + p;
                self.message = Some(format!("found '{}'", query));
                return true;
            }
        }
        for r in 0..=start_r {
            let line = &self.lines[r];
            if let Some(p) = line.to_lowercase().find(&q) {
                if r == start_r && p < start_c {
                    continue;
                }
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
