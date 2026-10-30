use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, Show},
    execute,
    style::ResetColor,
    terminal::{
        self, DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
    },
};

/// Restores the terminal when the editor returns, including on I/O errors.
pub(crate) struct TerminalSession;

impl TerminalSession {
    pub(crate) fn enter(output: &mut impl Write) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let session = Self;
        execute!(
            output,
            EnterAlternateScreen,
            Hide,
            DisableLineWrap,
            SetTitle("catbath")
        )?;
        Ok(session)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            ResetColor,
            LeaveAlternateScreen,
            Show,
            EnableLineWrap
        );
        let _ = terminal::disable_raw_mode();
    }
}
