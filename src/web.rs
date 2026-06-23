use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use crate::core::Editor;

pub fn run(path: &str) -> io::Result<()> {
    let path = PathBuf::from(path);
    let mut ed = Editor::new(path.clone())?;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let url = format!("http://{}", addr);

    println!(":: web editor ready at: {}", url);
    println!(" file: {}", path.display());
    println!(" (browser should open automatically. use ctrl+c to stop when finished editing)");

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
