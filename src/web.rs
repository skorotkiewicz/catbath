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

    // Serve plain HTML editor
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
<html><head><meta charset="utf-8"><title>editor: {fname}</title>
<style>
body{{margin:0;background:#111;color:#eee;font-family:monospace;height:100vh;display:flex;flex-direction:column}}
textarea{{flex:1;border:0;outline:0;background:#111;color:#eee;}}
</style>
</head>
<body>
<div style="padding:5px 10px;background:#222;color:#8f8">
<b>editor</b> - {fname} <span style="color:#888;font-size:12px">(ctrl+s to save)</span>
</div>
<textarea id="ed" spellcheck="false">{content}</textarea>
<script>
document.getElementById('ed').addEventListener('keydown',e={{
  if((e.ctrlKey||e.metaKey)&&e.key==='s'){{
    e.preventDefault();
    fetch('/save',{{method:'POST',body:'content='+encodeURIComponent(document.getElementById('ed').value)}})
      .then(()=>alert('saved'));
  }}
}});
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
