use std::{
    env,
    io::{self, Read, Write},
    process::{Command, Stdio},
};

pub fn run(key: &str, input: &str, file: &str, row: usize, col: usize) -> io::Result<String> {
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
