use std::{env, io::{self, Read, Write}, path::Path, process::{Command, Stdio}};

pub fn run(key: &str, input: &str, file: &str, row: usize, col: usize) -> io::Result<String> {
    let home = env::var("HOME").map_err(|_| io::Error::new(io::ErrorKind::Other, "HOME not set"))?;
    let ext_dir = format!("{}/.config/editor/extensions", home);

    // Find a script that matches the key (e.g., F1, F2)
    let script = std::fs::read_dir(&ext_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_name().map(|f| f == key).unwrap_or(false));

    if let Some(script) = script {
        let mut child = Command::new(&script)
            .env("EDITOR_FILE_PATH", file)
            .env("EDITOR_CURSOR_ROW", row.to_string())
            .env("EDITOR_CURSOR_COL", col.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        // Write text to script's stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes())?;
        }

        // Read modified text from script's stdout
        let mut out = String::new();
        child.stdout.take().unwrap().read_to_string(&mut out)?;

        child.wait()?;
        Ok(out)
    } else {
        // No script found? Just return the original text.
        Ok(input.to_string())
    }
}
