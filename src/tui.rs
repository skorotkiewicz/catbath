use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, /* DisableMouseCapture, EnableMouseCapture, */ Event, KeyCode, KeyEvent,
        KeyModifiers, MouseEventKind,
    },
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen, SetTitle,
    },
};

use crate::core::Editor;
use crate::extensions;

struct PendingExt {
    key: String,
    input: String,
    rx: Receiver<io::Result<String>>,
}

fn gutter(show_line_numbers: bool) -> usize {
    if show_line_numbers { 7 } else { 0 }
}

fn text_width(width: u16, show_line_numbers: bool) -> usize {
    (width as usize).saturating_sub(gutter(show_line_numbers))
}

fn hscroll(editor: &Editor, width: u16, show_line_numbers: bool) -> usize {
    editor
        .cursor_col
        .saturating_sub(text_width(width, show_line_numbers).saturating_sub(1))
}

pub fn render(
    editor: &Editor,
    stdout: &mut io::Stdout,
    width: u16,
    height: u16,
    show_line_numbers: bool,
) -> io::Result<()> {
    queue!(stdout, Hide)?;

    let vis = (height as usize).saturating_sub(2).max(1);
    let start = editor.scroll;
    let end = (start + vis).min(editor.lines.len());
    let gutter = gutter(show_line_numbers);
    let maxc = text_width(width, show_line_numbers);
    let hscroll = hscroll(editor, width, show_line_numbers);

    // Top bar
    let fname = editor
        .file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("untitled");
    let top = format!(
        " {} {}  |  {}  |  ^x quit  ^w save  ^l lines  ^z undo  ^k cut  ^u paste  ^f search",
        fname,
        if editor.modified { "(*)" } else { "" },
        editor.file_path.display()
    );
    let top = if top.len() > width as usize {
        &top[..width as usize]
    } else {
        &top
    };
    queue!(
        stdout,
        MoveTo(0, 0),
        SetBackgroundColor(Color::Rgb {
            r: 20,
            g: 40,
            b: 80
        }),
        SetForegroundColor(Color::White),
        Print(top)
    )?;
    if top.len() < width as usize {
        queue!(stdout, Print(" ".repeat(width as usize - top.len())))?;
    }
    queue!(stdout, ResetColor)?;

    // Lines
    for i in 0..vis {
        let y = i as u16 + 1;
        queue!(stdout, MoveTo(0, y), ResetColor)?;

        let idx = start + i;
        if idx >= end {
            queue!(stdout, Clear(ClearType::UntilNewLine))?;
            continue;
        }

        if show_line_numbers {
            let ln = format!(" {:>4}  ", idx + 1);
            queue!(
                stdout,
                MoveTo(0, y),
                SetForegroundColor(Color::DarkGrey),
                Print(&ln),
                ResetColor
            )?;
        }
        let line = &editor.lines[idx];
        let disp = if maxc == 0 || hscroll >= line.len() {
            ""
        } else if line.len() > hscroll + maxc {
            &line[hscroll..hscroll + maxc]
        } else {
            &line[hscroll..]
        };
        queue!(stdout, MoveTo(gutter as u16, y))?;
        editor.render_line(stdout, disp)?;
        queue!(stdout, ResetColor, Clear(ClearType::UntilNewLine))?;
    }

    queue_status(editor, stdout, width, height)?;

    // Cursor position
    let cy = 1u16 + (editor.cursor_row - editor.scroll) as u16;
    let cx = gutter + editor.cursor_col.saturating_sub(hscroll);
    if cy < height - 1 && cx < width as usize {
        let cx = cx as u16;
        queue!(stdout, MoveTo(cx, cy), Show)?;
    }
    stdout.flush()?;
    Ok(())
}

fn queue_status(
    editor: &Editor,
    stdout: &mut io::Stdout,
    width: u16,
    height: u16,
) -> io::Result<()> {
    let status = if let Some(m) = &editor.message {
        format!(
            " {}  |  Ln {}:{}  |  {} lines",
            m,
            editor.cursor_row + 1,
            editor.cursor_col + 1,
            editor.lines.len()
        )
    } else {
        format!(
            " Ln {}:{}  |  {} lines  |  {}",
            editor.cursor_row + 1,
            editor.cursor_col + 1,
            editor.lines.len(),
            if editor.modified { "modified" } else { "clean" }
        )
    };
    let st = if status.len() > width as usize {
        &status[..width as usize]
    } else {
        &status
    };
    queue!(
        stdout,
        MoveTo(0, height - 1),
        SetBackgroundColor(Color::DarkGrey),
        SetForegroundColor(Color::White),
        Print(st)
    )?;
    if st.len() < width as usize {
        queue!(stdout, Print(" ".repeat(width as usize - st.len())))?;
    }
    queue!(stdout, ResetColor)?;
    Ok(())
}

fn render_cursor(
    editor: &Editor,
    stdout: &mut io::Stdout,
    width: u16,
    height: u16,
    show_line_numbers: bool,
) -> io::Result<()> {
    let gutter = gutter(show_line_numbers);
    let hscroll = hscroll(editor, width, show_line_numbers);
    queue_status(editor, stdout, width, height)?;
    let cy = 1u16 + (editor.cursor_row - editor.scroll) as u16;
    let cx = gutter + editor.cursor_col.saturating_sub(hscroll);
    if cy < height - 1 && cx < width as usize {
        let cx = cx as u16;
        queue!(stdout, MoveTo(cx, cy), Show)?;
        stdout.flush()?;
    }
    Ok(())
}

pub fn run(path: &str) -> io::Result<()> {
    let mut ed = Editor::new(PathBuf::from(path))?;

    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        Hide,
        // EnableMouseCapture,
        DisableLineWrap,
        SetTitle("editor")
    )?;
    terminal::enable_raw_mode()?;

    let (mut w, mut h) = terminal::size()?;
    let mut vis = (h as usize).saturating_sub(2).max(1);
    ed.adjust_scroll(vis);
    let mut show_line_numbers = true;
    render(&ed, &mut stdout, w, h, show_line_numbers)?;

    let mut searching = false;
    let mut confirming_quit = false;
    let mut q = String::new();
    let mut last_search = String::new();
    let mut pending_ext: Option<PendingExt> = None;

    loop {
        if let Some(job) = pending_ext.take() {
            match job.rx.try_recv() {
                Ok(result) => {
                    finish_ext(&mut ed, &job.key, &job.input, result);
                    ed.adjust_scroll(vis);
                    render(&ed, &mut stdout, w, h, show_line_numbers)?;
                }
                Err(mpsc::TryRecvError::Empty) => pending_ext = Some(job),
                Err(mpsc::TryRecvError::Disconnected) => {
                    ed.message = Some(format!("ext err: {} worker stopped", job.key));
                    render(&ed, &mut stdout, w, h, show_line_numbers)?;
                }
            }
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(KeyEvent {
                    code, modifiers, ..
                }) => {
                    let before_move = (ed.cursor_row, ed.cursor_col, ed.scroll);
                    let before_hscroll = hscroll(&ed, w, show_line_numbers);
                    let mut cursor_move = false;

                    if confirming_quit {
                        match code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                if save_before_quit(&mut ed) {
                                    break;
                                }
                                confirming_quit = false;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') => break,
                            KeyCode::Esc => {
                                confirming_quit = false;
                                ed.message = Some("quit cancelled".into());
                            }
                            _ => {
                                ed.message = Some("save modified file before exit? (y/n)".into());
                            }
                        }
                    } else if searching {
                        match code {
                            KeyCode::Esc => {
                                searching = false;
                                q.clear();
                                ed.message = Some("search cancelled".into());
                            }
                            KeyCode::Enter => {
                                searching = false;
                                let query = if q.is_empty() { &last_search } else { &q };
                                if !query.is_empty() {
                                    ed.find_next(query);
                                    last_search = query.to_string();
                                }
                                q.clear();
                            }
                            KeyCode::Backspace => {
                                q.pop();
                                ed.message = Some(if q.is_empty() && !last_search.is_empty() {
                                    format!("search: [{}] (Enter=go, Esc=cancel)", last_search)
                                } else {
                                    format!("search: {} (Enter=go, Esc=cancel)", q)
                                });
                            }
                            KeyCode::Char(c) if !c.is_control() => {
                                q.push(c);
                                ed.message = Some(format!("search: {} (Enter=go, Esc=cancel)", q));
                            }
                            _ => {}
                        }
                    } else {
                        match (code, modifiers) {
                            (KeyCode::Char('x'), KeyModifiers::CONTROL) => {
                                if ed.modified {
                                    confirming_quit = true;
                                    ed.message =
                                        Some("save modified file before exit? (y/n)".into());
                                } else {
                                    break;
                                }
                            }
                            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                                let _ = ed.save();
                            }
                            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                                show_line_numbers = !show_line_numbers;
                                ed.message = Some(if show_line_numbers {
                                    "line numbers shown".into()
                                } else {
                                    "line numbers hidden".into()
                                });
                            }
                            (KeyCode::Char('z'), KeyModifiers::CONTROL) => ed.undo(),
                            (KeyCode::Char('k'), KeyModifiers::CONTROL) => ed.cut_line(),
                            (KeyCode::Char('u'), KeyModifiers::CONTROL) => ed.paste(),
                            (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                                searching = true;
                                q.clear();
                                ed.message = Some(if last_search.is_empty() {
                                    "search query: (type + Enter to find next)".into()
                                } else {
                                    format!(
                                        "search: [{}] (Enter=go, type=new, Esc=cancel)",
                                        last_search
                                    )
                                });
                            }
                            (KeyCode::Char(c), _) if !c.is_control() => ed.insert_char(c),
                            (KeyCode::Backspace, _) => ed.delete_char(),
                            (KeyCode::Delete, _) => ed.delete_forward(),
                            (KeyCode::Enter, _) => ed.insert_newline(),
                            (KeyCode::Up, _) => {
                                ed.move_up();
                                cursor_move = true;
                            }
                            (KeyCode::Down, _) => {
                                ed.move_down();
                                cursor_move = true;
                            }
                            (KeyCode::Left, _) => {
                                ed.move_left();
                                cursor_move = true;
                            }
                            (KeyCode::Right, _) => {
                                ed.move_right();
                                cursor_move = true;
                            }
                            (KeyCode::Home, _) => {
                                ed.move_home();
                                cursor_move = true;
                            }
                            (KeyCode::End, _) => {
                                ed.move_end();
                                cursor_move = true;
                            }
                            (KeyCode::PageUp, _) => ed.page_up(vis),
                            (KeyCode::PageDown, _) => ed.page_down(vis),
                            (KeyCode::Tab, _) => {
                                for _ in 0..4 {
                                    ed.insert_char(' ');
                                }
                            }
                            (KeyCode::F(n), _) => {
                                if pending_ext.is_some() {
                                    ed.message = Some("extension already running".into());
                                    continue;
                                }

                                let key = format!("F{}", n);
                                let input = ed.lines.join("\n");
                                let file = ed.file_path.to_string_lossy().into_owned();
                                let row = ed.cursor_row;
                                let col = ed.cursor_col;

                                ed.message = Some(format!("running {}...", key));
                                let (tx, rx) = mpsc::channel();
                                let thread_key = key.clone();
                                let thread_input = input.clone();
                                thread::spawn(move || {
                                    let result = extensions::run(
                                        &thread_key,
                                        &thread_input,
                                        &file,
                                        row,
                                        col,
                                    );
                                    let _ = tx.send(result);
                                });
                                pending_ext = Some(PendingExt { key, input, rx });
                            }
                            _ => {}
                        }
                    }
                    ed.adjust_scroll(vis);
                    let after_move = (ed.cursor_row, ed.cursor_col, ed.scroll);
                    let after_hscroll = hscroll(&ed, w, show_line_numbers);
                    if cursor_move {
                        if after_move == before_move {
                            continue;
                        }
                        if after_move.2 == before_move.2 && after_hscroll == before_hscroll {
                            render_cursor(&ed, &mut stdout, w, h, show_line_numbers)?;
                        } else {
                            render(&ed, &mut stdout, w, h, show_line_numbers)?;
                        }
                    } else {
                        render(&ed, &mut stdout, w, h, show_line_numbers)?;
                    }
                }
                Event::Mouse(m) => {
                    let gutter = if show_line_numbers { 7 } else { 0 };
                    if let MouseEventKind::Down(_) | MouseEventKind::Drag(_) = m.kind {
                        let r = (m.row as usize).saturating_sub(1) + ed.scroll;
                        if r < ed.lines.len() {
                            ed.cursor_row = r;
                            let c = (m.column as usize).saturating_sub(gutter);
                            ed.cursor_col = c.min(ed.lines[r].len());
                        }
                        ed.adjust_scroll(vis);
                        render(&ed, &mut stdout, w, h, show_line_numbers)?;
                    } else if let MouseEventKind::ScrollUp = m.kind {
                        ed.scroll = ed.scroll.saturating_sub(4);
                        render(&ed, &mut stdout, w, h, show_line_numbers)?;
                    } else if let MouseEventKind::ScrollDown = m.kind {
                        let mx = ed.lines.len().saturating_sub(vis);
                        ed.scroll = (ed.scroll + 4).min(mx);
                        render(&ed, &mut stdout, w, h, show_line_numbers)?;
                    }
                }
                Event::Resize(nw, nh) => {
                    w = nw;
                    h = nh;
                    vis = (h as usize).saturating_sub(2).max(1);
                    ed.adjust_scroll(vis);
                    render(&ed, &mut stdout, w, h, show_line_numbers)?;
                }
                _ => {}
            }
        }
    }

    execute!(
        stdout,
        LeaveAlternateScreen,
        Show,
        EnableLineWrap,
        // DisableMouseCapture
    )?;
    terminal::disable_raw_mode()?;
    Ok(())
}

fn save_before_quit(ed: &mut Editor) -> bool {
    match ed.save() {
        Ok(()) => true,
        Err(e) => {
            ed.message = Some(format!("save err: {}", e));
            false
        }
    }
}

fn finish_ext(ed: &mut Editor, key: &str, input: &str, result: io::Result<String>) {
    let out = match result {
        Ok(out) => out,
        Err(e) => {
            ed.message = Some(format!("ext err: {}", e));
            return;
        }
    };

    if ed.lines.join("\n") != input {
        ed.message = Some(if out == input {
            format!("{} returned no changes", key)
        } else {
            format!("{} finished; buffer changed, ignored", key)
        });
        return;
    }

    if out == input {
        ed.message = Some(format!("{} returned no changes", key));
        return;
    }

    ed.push_undo();
    ed.lines = if out.is_empty() {
        vec![String::new()]
    } else {
        out.split('\n').map(String::from).collect()
    };
    ed.cursor_row = ed.cursor_row.min(ed.lines.len().saturating_sub(1));
    ed.cursor_col = ed.cursor_col.min(ed.lines[ed.cursor_row].len());
    ed.modified = true;
    ed.message = Some(format!("{} applied", key));
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn extension_result_does_not_clobber_live_edits() {
        let mut ed = editor_with("hello");
        ed.lines = vec!["hello!".into()];

        finish_ext(&mut ed, "F1", "hello", Ok("HELLO".into()));

        assert_eq!(ed.lines, vec!["hello!"]);
        assert_eq!(
            ed.message.as_deref(),
            Some("F1 finished; buffer changed, ignored")
        );
    }
}
