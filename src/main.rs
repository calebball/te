/// Té is a simple modal text editor.
use std::io;

use crossterm::cursor;
use crossterm::{self, execute, terminal};

mod editor;
mod errors;

use crate::editor::Editor;
use crate::errors::Result;

fn main() -> Result<()> {
    let mut stdout = io::stdout();

    let filename = std::env::args().nth(1).expect("Did not receive filename");

    let (columns, rows) = terminal::size().expect("Failed to get terminal size");

    let mut editor = Editor::from_path(filename)?;
    editor.set_display_columns(columns);
    editor.set_display_rows(rows);

    execute!(stdout, terminal::EnterAlternateScreen).expect("Failed to enter alternate screen");
    terminal::enable_raw_mode().expect("Failed to enable raw mode");
    execute!(stdout, cursor::MoveTo(0, 0)).unwrap();

    editor.run(&mut stdout)?;

    terminal::disable_raw_mode().expect("Failed to disable raw mode");
    execute!(stdout, terminal::LeaveAlternateScreen).expect("Failed to leave alternate screen");

    Ok(())
}
