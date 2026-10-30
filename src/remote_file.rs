use std::io;
use std::process::Command;

use crate::command_output::command_output;

pub(crate) struct RemoteFile<'a> {
    target: &'a str,
    path: &'a str,
}

impl<'a> RemoteFile<'a> {
    pub(crate) fn parse(uri: &'a str) -> io::Result<Option<Self>> {
        let Some(rest) = uri.strip_prefix("ssh://") else {
            return Ok(None);
        };
        let Some(index) = rest.find('/') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "SSH URI needs an absolute file path",
            ));
        };
        let (target, path) = rest.split_at(index);
        if target.is_empty()
            || target.starts_with('-')
            || target.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid SSH destination",
            ));
        }
        if path.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "SSH path contains a NUL byte",
            ));
        }
        Ok(Some(Self { target, path }))
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new("ssh");
        // SSH joins remote arguments into a shell command. Quote the path for
        // that shell, and stop local option parsing before the destination.
        command.args([
            "--",
            self.target,
            &format!("{program} -- {}", shell_quote(self.path)),
        ]);
        command
    }

    pub(crate) fn load(&self) -> io::Result<String> {
        String::from_utf8(command_output(&mut self.command("cat"), b"")?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub(crate) fn save(&self, content: &str) -> io::Result<()> {
        command_output(&mut self.command("tee"), content.as_bytes())?;
        Ok(())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::RemoteFile;

    #[test]
    fn rejects_destinations_that_could_be_options() {
        for uri in [
            "ssh://-oProxyCommand=bad/file",
            "ssh:///file",
            "ssh://host",
            "ssh://bad host/file",
        ] {
            assert!(RemoteFile::parse(uri).is_err(), "must reject {uri:?}");
        }
    }

    #[test]
    fn quotes_remote_paths_as_one_shell_argument() {
        let remote = RemoteFile::parse("ssh://user@host/a b'c;$(echo bad)")
            .expect("valid URI")
            .expect("SSH URI");
        let command = remote.command("cat");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["--", "user@host", "cat -- '/a b'\\''c;$(echo bad)'"]);
    }
}
