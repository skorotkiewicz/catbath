use std::io;

const USAGE: &str = "usage: catbath [-g|-w] [--] <file>";

enum Mode {
    Terminal,
    Web,
}

struct Cli {
    mode: Mode,
    file: String,
}

impl Cli {
    fn run() -> io::Result<()> {
        let args = std::env::args_os()
            .skip(1)
            .map(|arg| {
                arg.into_string().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "arguments must be valid UTF-8")
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let Some(cli) = Self::parse(args)? else {
            println!("{USAGE}");
            return Ok(());
        };
        match cli.mode {
            Mode::Terminal => crate::tui::run(&cli.file),
            Mode::Web => crate::web::run(&cli.file),
        }
    }

    fn parse(args: impl IntoIterator<Item = String>) -> io::Result<Option<Self>> {
        let mut mode = Mode::Terminal;
        let mut file = None;
        let mut positional_only = false;
        for arg in args {
            if !positional_only {
                match arg.as_str() {
                    "-g" => {
                        mode = Mode::Terminal;
                        continue;
                    }
                    "-w" => {
                        mode = Mode::Web;
                        continue;
                    }
                    "-h" | "--help" => return Ok(None),
                    "--" => {
                        positional_only = true;
                        continue;
                    }
                    flag if flag.starts_with('-') => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("unknown option {flag:?}\n{USAGE}"),
                        ));
                    }
                    _ => {}
                }
            }
            if file.replace(arg).is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("only one file can be edited at a time\n{USAGE}"),
                ));
            }
        }
        let file = file.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, USAGE))?;
        Ok(Some(Self { mode, file }))
    }
}

/// Runs the editor and reports startup or runtime errors.
pub(crate) fn main() -> std::process::ExitCode {
    match Cli::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("catbath: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Mode};

    #[test]
    fn rejects_unknown_flags_missing_files_and_extra_files() {
        for args in [vec![], vec!["--unknown", "file"], vec!["one", "two"]] {
            assert!(
                Cli::parse(args.into_iter().map(String::from)).is_err(),
                "invalid arguments must fail"
            );
        }
    }

    #[test]
    fn supports_browser_mode_and_filenames_starting_with_a_dash() {
        let cli = Cli::parse(["-w", "--", "-file"].map(String::from))
            .expect("valid arguments")
            .expect("editing a file");
        assert!(
            matches!(cli.mode, Mode::Web),
            "-w selects the browser editor"
        );
        assert_eq!(cli.file, "-file");
    }
}
