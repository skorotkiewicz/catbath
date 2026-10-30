use std::{env, io, process::Command};

use crate::command_output::command_output;

pub(crate) fn is_function_key(key: &str) -> bool {
    matches!(
        key,
        "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12"
    )
}

pub fn run(key: &str, input: &str, file: &str, row: usize, col: usize) -> io::Result<String> {
    if !is_function_key(key) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "extension key must be F1 through F12",
        ));
    }
    let user_ext_dir = env::var("HOME")
        .ok()
        .map(|home| format!("{home}/.config/catbath/extensions"));

    let script = user_ext_dir
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("/usr/share/catbath/extensions"))
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(|entries| entries.filter_map(Result::ok).map(|e| e.path()))
        .find(|p| p.file_name().is_some_and(|f| f == key));

    if let Some(script) = script {
        let out = command_output(
            Command::new(&script)
                .env("EDITOR_FILE_PATH", file)
                .env("EDITOR_CURSOR_ROW", row.to_string())
                .env("EDITOR_CURSOR_COL", col.to_string()),
            input.as_bytes(),
        )?;
        String::from_utf8(out).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    } else {
        // No script found? Just return the original text.
        Ok(input.to_string())
    }
}
