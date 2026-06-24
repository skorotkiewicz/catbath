use crate::core::Editor;
use crate::extensions;
use std::io::{Read, Write};
use std::net::TcpListener;

const HTML: &str = include_str!("editor.html");

pub fn run(file: &str) -> std::io::Result<()> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    let port = l.local_addr()?.port();
    let url = format!("http://127.0.0.1:{port}");
    eprintln!("listening on {url}");
    open_browser(&url);
    serve(l, file.to_string())
}

pub fn serve(l: TcpListener, file: String) -> std::io::Result<()> {
    for stream in l.incoming() {
        let mut s = stream?;
        let mut buf = [0u8; 8192];
        let n = s.read(&mut buf)?;
        let req = String::from_utf8_lossy(&buf[..n]).into_owned();
        let (method, path) = req
            .lines()
            .next()
            .and_then(|l| {
                let mut p = l.split_whitespace();
                Some((p.next()?, p.next()?))
            })
            .unwrap_or(("", ""));

        let cl: usize = req
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);

        let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(n);
        let mut body = buf[body_start..n].to_vec();
        while body.len() < cl {
            let m = s.read(&mut buf)?;
            if m == 0 {
                break;
            }
            body.extend_from_slice(&buf[..m]);
        }

        let (code, ct, b) = match (method, path) {
            ("GET", p) if p == "/" || p.starts_with("/?") => {
                let content = Editor::load(&file).unwrap_or_default();
                // Lazy escaping: only & and <. This is 100% safe for <textarea>.
                let esc_content = content.replace('&', "&amp;").replace('<', "&lt;");
                let esc_fname = file.replace('<', "&lt;");

                let html = HTML
                    .replace("{fname}", &esc_fname)
                    .replace("{content}", &esc_content);

                ("200 OK", "text/html; charset=utf-8", html.into_bytes())
            }
            ("POST", p) if p.starts_with("/save") => {
                let _ = Editor::save_to(&file, std::str::from_utf8(&body).unwrap_or(""));
                ("200 OK", "text/plain", b"ok".to_vec())
            }
            ("POST", p) if p.starts_with("/ext/") => {
                let key = p.trim_start_matches("/ext/"); // e.g., "F1"
                let input = std::str::from_utf8(&body).unwrap_or("");
                let out =
                    extensions::run(key, input, &file, 0, 0).unwrap_or_else(|e| e.to_string());
                ("200 OK", "text/plain", out.into_bytes())
            }
            _ => ("404 Not Found", "text/plain", b"404".to_vec()),
        };

        let h = format!(
            "HTTP/1.1 {code}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
            b.len()
        );
        s.write_all(h.as_bytes())?;
        s.write_all(&b)?;
        s.flush()?;
    }
    Ok(())
}

#[rustfmt::skip]
fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]  { let _ = std::process::Command::new("open").arg(url).spawn(); }
    #[cfg(target_os = "linux")]  { let _ = std::process::Command::new("xdg-open").arg(url).spawn(); }
    #[cfg(target_os = "windows")] { let _ = std::process::Command::new("cmd").args(["/C","start",url]).spawn(); }
}
