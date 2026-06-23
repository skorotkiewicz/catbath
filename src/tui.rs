use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseEventKind,
    },
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen, SetTitle,
    },
};

use crate::core::Editor;

pub fn render(editor: &Editor, stdout: &mut io::Stdout, width: u16, height: u16) -> io::Result<()> {
    queue!(stdout, Clear(ClearType::All))?;

    let vis = (height as usize).saturating_sub(2).max(1);
    let start = editor.scroll;
    let end = (start + vis).min(editor.lines.len());

    // Top bar
    let fname = editor
        .file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("untitled");
    let top = format!(
        " {} {}  •  {}  •  ctrl+q quit  ctrl+s save  ctrl+z undo  ctrl+f search",
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
        Print(top),
        ResetColor
    )?;
    if top.len() < width as usize {
        queue!(stdout, Print(" ".repeat(width as usize - top.len())))?;
    }

    // Lines
    for (i, idx) in (start..end).enumerate() {
        let y = i as u16 + 1;
        let ln = format!(" {:>4} ", idx + 1);
        queue!(
            stdout,
            MoveTo(0, y),
            SetForegroundColor(Color::DarkGrey),
            Print(&ln),
            ResetColor
        )?;
        let line = &editor.lines[idx];
        let maxc = (width as usize).saturating_sub(7);
        let disp = if line.len() > maxc {
            &line[..maxc]
        } else {
            line
        };
        queue!(stdout, MoveTo(7, y), Print(disp))?;
    }

    // Bottom bar
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
        Print(st),
        ResetColor
    )?;

    // Cursor position
    let cy = 1u16 + (editor.cursor_row - editor.scroll) as u16;
    let cx = 7u16 + editor.cursor_col as u16;
    if cy < height - 1 {
        queue!(stdout, MoveTo(cx, cy), Show)?;
    }
    stdout.flush()?;
    Ok(())
}

pub fn run(path: &str) -> io::Result<()> {
    let mut ed = Editor::new(PathBuf::from(path))?;

    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        Hide,
        EnableMouseCapture,
        DisableLineWrap,
        SetTitle("editor")
    )?;
    terminal::enable_raw_mode()?;

    let (mut w, mut h) = terminal::size()?;
    let mut vis = (h as usize).saturating_sub(2).max(1);
    ed.adjust_scroll(vis);
    render(&ed, &mut stdout, w, h)?;

    let mut searching = false;
    let mut q = String::new();

    loop {
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(KeyEvent {
                    code, modifiers, ..
                }) => {
                    if searching {
                        match code {
                            KeyCode::Esc => {
                                searching = false;
                                q.clear();
                                ed.message = Some("search cancelled".into());
                            }
                            KeyCode::Enter => {
                                searching = false;
                                if !q.is_empty() {
                                    ed.find_next(&q);
                                }
                                q.clear();
                            }
                            KeyCode::Backspace => {
                                q.pop();
                                ed.message = Some(format!("search: {} (Enter=go, Esc=cancel)", q));
                            }
                            KeyCode::Char(c) if !c.is_control() => {
                                q.push(c);
                                ed.message = Some(format!("search: {} (Enter=go, Esc=cancel)", q));
                            }
                            _ => {}
                        }
                    } else {
                        match (code, modifiers) {
                            (KeyCode::Char('q'), KeyModifiers::CONTROL) => break,
                            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                                let _ = ed.save();
                            }
                            (KeyCode::Char('z'), KeyModifiers::CONTROL) => ed.undo(),
                            (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                                searching = true;
                                q.clear();
                                ed.message =
                                    Some("search query: (type + Enter to find next)".into());
                            }
                            (KeyCode::Char(c), _) if !c.is_control() => ed.insert_char(c),
                            (KeyCode::Backspace, _) => ed.delete_char(),
                            (KeyCode::Delete, _) => ed.delete_forward(),
                            (KeyCode::Enter, _) => ed.insert_newline(),
                            (KeyCode::Up, _) => ed.move_up(),
                            (KeyCode::Down, _) => ed.move_down(),
                            (KeyCode::Left, _) => ed.move_left(),
                            (KeyCode::Right, _) => ed.move_right(),
                            (KeyCode::Home, _) => ed.move_home(),
                            (KeyCode::End, _) => ed.move_end(),
                            (KeyCode::PageUp, _) => ed.page_up(vis),
                            (KeyCode::PageDown, _) => ed.page_down(vis),
                            (KeyCode::Tab, _) => {
                                for _ in 0..4 {
                                    ed.insert_char(' ');
                                }
                            }
                            _ => {}
                        }
                    }
                    ed.adjust_scroll(vis);
                    render(&ed, &mut stdout, w, h)?;
                }
                Event::Mouse(m) => {
                    if let MouseEventKind::Down(_) | MouseEventKind::Drag(_) = m.kind {
                        let r = (m.row as usize).saturating_sub(1) + ed.scroll;
                        if r < ed.lines.len() {
                            ed.cursor_row = r;
                            let c = (m.column as usize).saturating_sub(7);
                            ed.cursor_col = c.min(ed.lines[r].len());
                        }
                        ed.adjust_scroll(vis);
                        render(&ed, &mut stdout, w, h)?;
                    } else if let MouseEventKind::ScrollUp = m.kind {
                        ed.scroll = ed.scroll.saturating_sub(4);
                        render(&ed, &mut stdout, w, h)?;
                    } else if let MouseEventKind::ScrollDown = m.kind {
                        let mx = ed.lines.len().saturating_sub(vis);
                        ed.scroll = (ed.scroll + 4).min(mx);
                        render(&ed, &mut stdout, w, h)?;
                    }
                }
                Event::Resize(nw, nh) => {
                    w = nw;
                    h = nh;
                    vis = (h as usize).saturating_sub(2).max(1);
                    ed.adjust_scroll(vis);
                    render(&ed, &mut stdout, w, h)?;
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
        DisableMouseCapture
    )?;
    terminal::disable_raw_mode()?;
    println!("+ done. file: {}", ed.file_path.display());
    if ed.modified {
        println!("   (had unsaved changes on exit)");
    }
    Ok(())
}
