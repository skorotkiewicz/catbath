use std::io::{self, Write};
use std::process::{Command, Stdio};

/// Runs a process while feeding its input and draining both output pipes.
pub(crate) fn command_output(command: &mut Command, input: &[u8]) -> io::Result<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .expect("the child was spawned with piped stdin");

    std::thread::scope(|scope| {
        // A filter may write before it reads. Drain output while sending input
        // so neither side can block on a full pipe.
        let writer = scope.spawn(move || stdin.write_all(input));
        let output = child.wait_with_output();
        let written = writer
            .join()
            .map_err(|_| io::Error::other("process input worker panicked"))?;
        let output = output?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "{} exited with {}: {}",
                command.get_program().to_string_lossy(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        written?;
        Ok(output.stdout)
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::command_output;
    use std::process::Command;

    #[test]
    fn nonzero_status_is_an_error_even_with_output() {
        let error = command_output(
            Command::new("sh").args(["-c", "printf partial; printf failed >&2; exit 7"]),
            b"",
        )
        .expect_err("failed filters must not replace the buffer");
        assert!(
            error.to_string().contains("failed"),
            "include the process diagnostic"
        );
    }

    #[test]
    fn drains_output_while_sending_large_input() {
        // Both directions exceed typical pipe capacity. The child writes first.
        let input = vec![b'x'; 1024 * 1024];
        let output = command_output(
            Command::new("sh").args(["-c", "head -c 1048576 /dev/zero; cat"]),
            &input,
        )
        .expect("large process pipes must make progress");
        assert_eq!(output.len(), 2 * input.len());
        assert_eq!(&output[input.len()..], input);
    }
}
