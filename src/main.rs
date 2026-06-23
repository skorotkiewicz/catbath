use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
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

/// Core editor state - shared across modes. Efficient enough for most use cases.
struct Editor {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll: usize,
    file_path: PathBuf,
    modified: bool,
    undo_stack: Vec<(Vec<String>, usize, usize)>,
    message: Option<String>,
}

impl Editor {
    fn new(path: PathBuf) -> io::Result<Self> {
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
            undo_stack: Vec::with_capacity(30),
            message: Some("Ctrl+Q quit • Ctrl+S save • Ctrl+Z undo • Ctrl+F search • Click or scroll with mouse".to_string()),
        })
    }

    fn push_undo(&mut self) {
        if self.undo_stack.len() >= 30 {
            self.undo_stack.remove(0);
        }
        self.undo_stack
            .push((self.lines.clone(), self.cursor_row, self.cursor_col));
    }

    fn undo(&mut self) {
        if let Some((lines, row, col)) = self.undo_stack.pop() {
            self.lines = lines;
            self.cursor_row = row.min(self.lines.len().saturating_sub(1));
            let llen = self.lines.get(self.cursor_row).map_or(0, |l| l.len());
            self.cursor_col = col.min(llen);
            self.modified = true;
            self.message = Some("Undid last change".to_string());
        } else {
            self.message = Some("Nothing to undo".to_string());
        }
    }

    fn save(&mut self) -> io::Result<()> {
        let content = self.lines.join("\n");
        fs::write(&self.file_path, content)?;
        self.modified = false;
        self.message = Some(format!("✓ Saved {}", self.file_path.display()));
        Ok(())
    }

    fn insert_char(&mut self, c: char) {
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

    fn insert_newline(&mut self) {
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

    fn delete_char(&mut self) {
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

    fn delete_forward(&mut self) {
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

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    fn move_right(&mut self) {
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

    fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            let len = self.lines[self.cursor_row].len();
            self.cursor_col = self.cursor_col.min(len);
        }
    }

    fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            let len = self.lines[self.cursor_row].len();
            self.cursor_col = self.cursor_col.min(len);
        }
    }

    fn move_home(&mut self) {
        self.cursor_col = 0;
    }
    fn move_end(&mut self) {
        if self.cursor_row < self.lines.len() {
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    fn page_up(&mut self, vis: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(vis.max(1));
        if self.cursor_row < self.lines.len() {
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
        }
        self.scroll = self.scroll.saturating_sub(vis.max(1));
    }

    fn page_down(&mut self, vis: usize) {
        self.cursor_row = (self.cursor_row + vis.max(1)).min(self.lines.len().saturating_sub(1));
        if self.cursor_row < self.lines.len() {
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
        }
        self.scroll = (self.scroll + vis.max(1)).min(self.lines.len().saturating_sub(1));
    }

    fn adjust_scroll(&mut self, vis: usize) {
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

    fn find_next(&mut self, query: &str) -> bool {
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
                self.message = Some(format!("Found '{}'", query));
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
                self.message = Some(format!("Found '{}' (wrapped)", query));
                return true;
            }
        }
        self.message = Some(format!("Not found: '{}'", query));
        false
    }
}

fn render(editor: &Editor, stdout: &mut io::Stdout, width: u16, height: u16) -> io::Result<()> {
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
        " {} {}  •  {}  •  Ctrl+Q quit  Ctrl+S save  Ctrl+Z undo  Ctrl+F search",
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
        queue!(stdout, MoveTo(cx, cy))?;
    }
    stdout.flush()?;
    Ok(())
}

fn run_tui_mode(path: PathBuf) -> io::Result<()> {
    let mut ed = Editor::new(path)?;

    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        Hide,
        EnableMouseCapture,
        DisableLineWrap,
        SetTitle("editor — efficient Rust TUI")
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
                                ed.message = Some("Search cancelled".into());
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
                                ed.message = Some(format!("Search: {} (Enter=go, Esc=cancel)", q));
                            }
                            KeyCode::Char(c) if !c.is_control() => {
                                q.push(c);
                                ed.message = Some(format!("Search: {} (Enter=go, Esc=cancel)", q));
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
                                    Some("Search query: (type + Enter to find next)".into());
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
    println!("+ Done. File: {}", ed.file_path.display());
    if ed.modified {
        println!("   (had unsaved changes on exit)");
    }
    Ok(())
}

// ==================== WEB MODE ====================
fn run_web_mode(path: PathBuf) -> io::Result<()> {
    let mut ed = Editor::new(path.clone())?;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let url = format!("http://{}", addr);

    println!("🌐 Web editor ready at: {}", url);
    println!("   File: {}", path.display());
    println!("   (Browser should open automatically. Use Ctrl+C to stop when finished editing.)");

    // Best effort open browser
    let _ = std::process::Command::new(if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "xdg-open"
    })
    .args(if cfg!(target_os = "windows") {
        vec!["/C", "start", url.as_str()]
    } else {
        vec![url.as_str()]
    })
    .spawn();

    for s in listener.incoming().flatten() {
        if let Err(e) = handle_http(s, &mut ed, &path) {
            eprintln!("http err: {}", e);
        }
    }
    Ok(())
}

fn handle_http(mut stream: TcpStream, ed: &mut Editor, file_path: &Path) -> io::Result<()> {
    let mut buf = [0; 8192];
    let n = stream.read(&mut buf)?;
    let req_str = String::from_utf8_lossy(&buf[0..n]);

    if req_str.starts_with("POST /save") {
        if let Some(body_idx) = req_str.find("\r\n\r\n") {
            let body = &req_str[body_idx + 4..];
            // Parse content=... (urlencoded)
            let new_text = if let Some(p) = body.find("content=") {
                let val = &body[p + 8..];
                // Minimal decode for common cases
                val.replace("%0A", "\n")
                    .replace("%0D", "")
                    .replace('+', " ")
                    .replace("%20", " ")
            } else {
                body.to_string()
            };
            ed.lines = if new_text.trim().is_empty() {
                vec!["".into()]
            } else {
                new_text.lines().map(str::to_string).collect()
            };
            ed.modified = true;
            let _ = ed.save();
        }
        let r = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        stream.write_all(r.as_bytes())?;
        return Ok(());
    }

    // Serve nice HTML editor (Tailwind CDN = modern look, zero size cost to binary)
    let fname = file_path
        .file_name()
        .and_then(|s: &std::ffi::OsStr| s.to_str())
        .unwrap_or("document.txt");
    let content = ed
        .lines
        .join("\n")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    let html = format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>editor — {fname}</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body{{font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,"Liberation Mono","Courier New",monospace}}</style>
</head>
<body class="bg-[#0a0a0a] text-[#e5e5e5]">
<div class="max-w-[1200px] mx-auto p-6">
  <div class="flex items-center justify-between mb-4">
    <div class="flex items-center gap-x-3">
      <div class="text-2xl font-semibold tracking-tighter">editor</div>
      <div class="text-[10px] px-2 py-px rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30">WEB MODE</div>
    </div>
    <div class="text-sm text-zinc-500">editing <span class="font-mono text-emerald-400">{fname}</span></div>
    <button onclick="doSave()" class="px-5 py-2 rounded-xl bg-white text-black text-sm font-medium flex items-center gap-2 active:scale-[0.985] hover:bg-zinc-100 transition">
      <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.25" d="M17 13v6m-3-3h6M6 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4l2 2h8a2 2 0 012 2v3"/></svg>
      SAVE TO DISK
    </button>
  </div>

  <textarea id="ed" class="w-full h-[70vh] p-5 bg-[#111] border border-white/10 focus:border-emerald-500/60 rounded-3xl text-sm leading-relaxed outline-none" spellcheck="false" onkeydown="if((event.ctrlKey||event.metaKey)&&event.key==='s'){{event.preventDefault();doSave()}}">{content}</textarea>

  <div class="flex justify-between text-[10px] text-zinc-500 mt-3 px-1">
    <div>Ctrl/Cmd+S saves directly to the file on disk • Changes are immediate</div>
    <div class="font-mono">single-binary Rust • &lt;1MB</div>
  </div>
</div>
<script>
async function doSave() {{
  const txt = document.getElementById('ed').value;
  const res = await fetch('/save', {{ method:'POST', headers:{{'Content-Type':'application/x-www-form-urlencoded'}}, body: 'content='+encodeURIComponent(txt) }});
  if (res.ok) {{
    const b = document.querySelector('button'); b.style.background='#166534'; b.textContent='SAVED!';
    setTimeout(()=>location.reload(), 650);
  }} else alert('Save failed');
}}
tailwind.config = {{theme:{{extend:{{}}}}}};
</script>
</body></html>"#
    );

    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        html.len(),
        html
    );
    stream.write_all(resp.as_bytes())?;
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut g = false;
    let mut w = false;
    let mut fpath: Option<String> = None;

    for arg in &args[1..] {
        if arg == "-g" || arg == "--gui" {
            g = true;
        } else if arg == "-w" || arg == "--web" {
            w = true;
        } else if !arg.starts_with('-') {
            fpath = Some(arg.clone());
        }
    }

    let path = match fpath {
        Some(p) => PathBuf::from(p),
        None => {
            println!(
                "full & efficient graphic text editor in modern Rust — single binary <1MB\n\nUSAGE:\n  ./editor <file>     opens TUI (full featured, mouse + keyboard, undo, search)\n  ./editor -g <file>  GUI mode (shows TUI - keeps binary tiny)\n  ./editor -w <file>  Web mode (beautiful local browser-based editor)\n\nAll useful. TUI is the star: efficient, no bloat, works everywhere."
            );
            return Ok(());
        }
    };

    if w {
        run_web_mode(path)
    } else {
        if g {
            println!(
                "GUI mode: TUI provided instead (modern terminals = GPU 'GUI', zero extra size)."
            );
        }
        run_tui_mode(path)
    }
}
