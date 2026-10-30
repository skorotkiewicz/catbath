use unicode_width::UnicodeWidthChar;

/// Maps UTF-8 byte offsets to terminal columns and renders control characters safely.
pub(crate) struct DisplayLine<'a> {
    text: &'a str,
}

impl<'a> DisplayLine<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub(crate) fn column_at(&self, byte: usize) -> usize {
        let mut column = 0;
        for (offset, c) in self.text.char_indices() {
            if offset >= byte {
                break;
            }
            column += char_width(c, column);
        }
        column
    }

    pub(crate) fn byte_at(&self, column: usize) -> usize {
        let mut current = 0;
        for (byte, c) in self.text.char_indices() {
            let end = current + char_width(c, current);
            if column < end {
                return byte;
            }
            current = end;
        }
        self.text.len()
    }

    pub(crate) fn slice(&self, start: usize, width: usize) -> String {
        let limit = start.saturating_add(width);
        let mut output = String::new();
        let mut column = 0;
        for c in self.text.chars() {
            let end = column + char_width(c, column);
            if end == column {
                if !output.is_empty() {
                    output.push(c);
                }
                continue;
            }
            if column >= limit {
                break;
            }
            if end > start {
                if c == '\t' || column < start || end > limit {
                    output.push_str(&" ".repeat(end.min(limit) - column.max(start)));
                } else if c.is_control() {
                    output.push('�');
                } else {
                    output.push(c);
                }
            }
            column = end;
        }
        output
    }
}

fn char_width(c: char, column: usize) -> usize {
    if c == '\t' {
        4 - column % 4
    } else if c.is_control() {
        1
    } else {
        c.width().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::DisplayLine;

    #[test]
    fn maps_unicode_bytes_to_display_columns() {
        let line = DisplayLine::new("é界e\u{301}x");
        assert_eq!(line.column_at("é界e\u{301}".len()), 4);
        assert_eq!(line.byte_at(2), "é".len());
        assert_eq!(line.byte_at(4), "é界e\u{301}".len());
        assert_eq!(line.slice(0, 3), "é界");
        assert_eq!(line.slice(2, 3), " e\u{301}x");
    }

    #[test]
    fn expands_tabs_and_never_emits_terminal_control_characters() {
        let line = DisplayLine::new("a\tb\x1b[2J\r");
        assert_eq!(line.column_at(2), 4);
        assert_eq!(line.byte_at(3), 1);
        assert_eq!(line.slice(0, 20), "a   b�[2J�");
        assert_eq!(line.slice(2, 3), "  b");
    }

    #[test]
    fn clips_wide_characters_without_overrunning_the_viewport() {
        let line = DisplayLine::new("界x");
        assert_eq!(line.slice(0, 1), " ");
        assert_eq!(line.slice(1, 2), " x");
        assert_eq!(line.slice(0, 0), "");
    }
}
