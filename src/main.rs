//! A small terminal and browser text editor.
#![warn(missing_docs)]

mod cli;
mod command_output;
mod core;
mod display_line;
mod extensions;
mod remote_file;
mod syntax;
mod terminal_session;
#[cfg(test)]
mod test_directory;
mod tui;
mod web;

pub(crate) use cli::main;
