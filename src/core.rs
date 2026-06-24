use std::fs;
use std::io;
use std::path::PathBuf;

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
}

impl Editor {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let lines = if path.exists() {
            let content = fs::read_to_string(&path)?;
            if content.is_empty() {
                vec!["".to_string()]
            } else {
                content.lines().map(str::to_string).collect()
            }
        } else {
            vec!["".to_string()]
        };
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
            message: Some("^x quit | ^w save | ^z undo | ^k cut | ^u paste | ^f search | click or scroll with mouse".to_string()),
        })
    }

    fn push_undo(&mut self) {
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

    pub fn save(&mut self) -> io::Result<()> {
        let content = self.lines.join("\n");
        fs::write(&self.file_path, content)?;
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
        if self.cursor_row >= self.lines.len() { return; }

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
        if self.clip_lines.is_empty() { return; }
        self.push_undo();

        // O(N) block insertion
        self.lines.splice(
            self.cursor_row..self.cursor_row,
            self.clip_lines.iter().cloned()
        );

        self.cursor_row += self.clip_lines.len();
        self.cursor_col = 0;
        self.modified = true;
        self.last_op_was_cut = false; // Pasting breaks the cut sequence
    }
}
