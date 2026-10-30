use std::io::{self, Cursor, Read};
use std::net::TcpListener;

use tiny_http::{Header, Method, Request, Response, Server};

use crate::core::Editor;
use crate::extensions;

const HTML: &str = include_str!("editor.html");
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug)]
struct BodyError {
    status: u16,
    message: String,
}

impl BodyError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

pub(crate) fn run(file: &str) -> io::Result<()> {
    Editor::load(file)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}", listener.local_addr()?);
    eprintln!("listening on {url}");
    open_browser(&url);
    serve(listener, file)
}

fn serve(listener: TcpListener, file: &str) -> io::Result<()> {
    let host = listener.local_addr()?.to_string();
    let server = Server::from_listener(listener, None).map_err(io::Error::other)?;
    for mut request in server.incoming_requests() {
        let response = handle_request(&mut request, file, &host);
        // A disconnected client must not stop the editor server.
        if let Err(error) = request.respond(response) {
            eprintln!("response error: {error}");
        }
    }
    Ok(())
}

fn handle_request(request: &mut Request, file: &str, host: &str) -> Response<Cursor<Vec<u8>>> {
    let origin = format!("http://{host}");
    if header_value(request, "Host") != Some(host)
        || header_value(request, "Origin").is_some_and(|value| value != origin)
        || header_value(request, "Sec-Fetch-Site") == Some("cross-site")
    {
        return text_response(403, "request origin rejected");
    }

    if request.method() == &Method::Get && request.url().split('?').next() == Some("/") {
        return match Editor::load(file) {
            Ok(content) => response(200, "text/html; charset=utf-8", render_html(file, &content)),
            Err(error) => text_response(500, error.to_string()),
        };
    }

    if request.method() != &Method::Post {
        return text_response(404, "not found");
    }
    // Browsers cannot send this custom header cross-origin without a preflight.
    // No CORS permission is granted by this server.
    if header_value(request, "X-Catbath") != Some("1") {
        return text_response(403, "missing editor request header");
    }
    let key = request.url().strip_prefix("/ext/");
    if request.url() != "/save" && !key.is_some_and(extensions::is_function_key) {
        return text_response(404, "not found");
    }
    let key = key.map(str::to_owned);
    let length = request.body_length();
    let body = match read_body(request.as_reader(), length) {
        Ok(body) => body,
        Err(error) => return text_response(error.status, error.message),
    };

    let result = if let Some(key) = key {
        extensions::run(&key, &body, file, 0, 0)
    } else {
        Editor::save_to(file, &body).map(|()| "ok".to_string())
    };
    match result {
        Ok(output) => text_response(200, output),
        Err(error) => text_response(500, error.to_string()),
    }
}

fn header_value<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    let mut values = request
        .headers()
        .iter()
        .filter(|header| header.field.equiv(name));
    let value = values.next()?.value.as_str();
    if values.next().is_some() {
        return None;
    }
    Some(value)
}

fn read_body(
    reader: &mut (impl Read + ?Sized),
    length: Option<usize>,
) -> Result<String, BodyError> {
    if length.is_some_and(|length| length > MAX_BODY_BYTES) {
        return Err(BodyError::new(413, "buffer exceeds the 16 MiB web limit"));
    }
    let mut body = Vec::new();
    reader
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| BodyError::new(400, error.to_string()))?;
    if body.len() > MAX_BODY_BYTES {
        return Err(BodyError::new(413, "buffer exceeds the 16 MiB web limit"));
    }
    if length.is_some_and(|length| length != body.len()) {
        return Err(BodyError::new(400, "incomplete request body"));
    }
    String::from_utf8(body).map_err(|error| BodyError::new(400, error.to_string()))
}

fn render_html(file: &str, content: &str) -> String {
    // Substitute each field only once. File contents may contain placeholders.
    HTML.split("{content}")
        .map(|part| part.replace("{fname}", &escape_html(file)))
        .collect::<Vec<_>>()
        .join(&escape_html(content))
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn text_response(status: u16, body: impl Into<Vec<u8>>) -> Response<Cursor<Vec<u8>>> {
    response(status, "text/plain; charset=utf-8", body)
}

fn response(
    status: u16,
    content_type: &'static str,
    body: impl Into<Vec<u8>>,
) -> Response<Cursor<Vec<u8>>> {
    let mut response = Response::from_data(body.into()).with_status_code(status);
    for (name, value) in [
        ("Content-Type", content_type),
        ("Cache-Control", "no-store"),
        ("X-Content-Type-Options", "nosniff"),
        (
            "Content-Security-Policy",
            "frame-ancestors 'none'; base-uri 'none'",
        ),
    ] {
        response.add_header(
            Header::from_bytes(name, value).expect("response headers are valid ASCII constants"),
        );
    }
    response
}

#[rustfmt::skip]
fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]  { let _ = std::process::Command::new("open").arg(url).spawn(); }
    #[cfg(target_os = "linux")]  { let _ = std::process::Command::new("xdg-open").arg(url).spawn(); }
    #[cfg(target_os = "windows")] { let _ = std::process::Command::new("cmd").args(["/C","start",url]).spawn(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_directory::TestDirectory;
    use tiny_http::{StatusCode, TestRequest};

    const HOST: &str = "127.0.0.1:12345";

    fn request(method: Method, path: &str, body: &'static str) -> Request {
        TestRequest::new()
            .with_method(method)
            .with_path(path)
            .with_body(body)
            .with_header(Header::from_bytes("Host", HOST).expect("valid host header"))
            .with_header(Header::from_bytes("X-Catbath", "1").expect("valid editor header"))
            .into()
    }

    #[test]
    fn save_reports_failure_and_matches_only_the_exact_route() {
        let dir = TestDirectory::new();
        let file = dir.path().join("missing/file.txt");
        let file = file.to_str().expect("test path is UTF-8");
        assert_eq!(
            handle_request(&mut request(Method::Post, "/save", "text"), file, HOST).status_code(),
            StatusCode(500)
        );
        assert_eq!(
            handle_request(
                &mut request(Method::Post, "/save-anything", "text"),
                file,
                HOST
            )
            .status_code(),
            StatusCode(404)
        );
    }

    #[test]
    fn save_writes_the_complete_utf8_buffer() {
        let dir = TestDirectory::new();
        let file = dir.path().join("file.txt");
        let response = handle_request(
            &mut request(Method::Post, "/save", "é\n\n"),
            file.to_str().expect("UTF-8 path"),
            HOST,
        );
        assert_eq!(response.status_code(), StatusCode(200));
        assert_eq!(
            std::fs::read_to_string(file).expect("read saved file"),
            "é\n\n"
        );
    }

    #[test]
    fn rejects_cross_origin_writes_and_rebound_hosts() {
        let dir = TestDirectory::new();
        let file = dir.path().join("untouched.txt");
        let file_str = file.to_str().expect("UTF-8 path");
        let mut cross_origin: Request = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/save")
            .with_body("bad")
            .with_header(Header::from_bytes("Host", HOST).expect("valid header"))
            .into();
        assert_eq!(
            handle_request(&mut cross_origin, file_str, HOST).status_code(),
            StatusCode(403)
        );
        let mut rebound = request(Method::Post, "/save", "bad");
        assert_eq!(
            handle_request(&mut rebound, file_str, "other-host").status_code(),
            StatusCode(403)
        );
        assert!(!file.exists(), "rejected requests must not write a file");
    }

    #[test]
    fn rejects_unsupported_extension_keys() {
        let mut req = request(Method::Post, "/ext/other-script", "text");
        assert_eq!(
            handle_request(&mut req, "unused", HOST).status_code(),
            StatusCode(404)
        );
    }

    #[test]
    fn body_reader_handles_large_input_and_rejects_truncation_or_invalid_utf8() {
        let input = "é".repeat(32 * 1024);
        assert_eq!(
            read_body(&mut input.as_bytes(), Some(input.len())).expect("complete body"),
            input
        );
        assert_eq!(
            read_body(&mut &b"short"[..], Some(10))
                .expect_err("truncated body")
                .status,
            400
        );
        assert_eq!(
            read_body(&mut &[0xff][..], Some(1))
                .expect_err("invalid UTF-8")
                .status,
            400
        );
        assert_eq!(
            read_body(&mut &b""[..], Some(MAX_BODY_BYTES + 1))
                .expect_err("body limit")
                .status,
            413
        );
    }

    #[test]
    fn html_escapes_both_fields_without_replacing_content_placeholders() {
        let html = render_html(
            "&lt;{content}</title>",
            "{fname}</textarea><script>bad</script>",
        );
        assert!(
            html.contains("&amp;lt;{content}&lt;/title&gt;"),
            "escape the filename once"
        );
        assert!(
            html.contains("{fname}&lt;/textarea&gt;&lt;script&gt;bad&lt;/script&gt;"),
            "escape file contents once"
        );
    }
}
