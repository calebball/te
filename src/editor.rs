use std::cmp;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crossterm::event::{Event, KeyCode, KeyEvent};
use crossterm::tty::IsTty;
use crossterm::{cursor, event, queue, terminal};

use crate::errors::{EditorError, Result};

/// The different modes that Té currently provides.
#[derive(Debug)]
enum EditorMode {
    /// Navigation mode provides movement through the document.
    Navigate,
    /// Edit mode allows for insertion and removal of text in the document (like Vim's insert mode.)
    Edit,
}

impl Default for EditorMode {
    fn default() -> Self {
        EditorMode::Navigate
    }
}

#[derive(Debug)]
struct DisplaySize {
    columns: u16,
    rows: u16,
}

impl DisplaySize {
    fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }
}

impl Default for DisplaySize {
    fn default() -> Self {
        DisplaySize::new(80, 24)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct DisplayPosition {
    column: usize,
    row: usize,
}

impl DisplayPosition {
    fn new(column: usize, row: usize) -> Self {
        Self { column, row }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct CursorPosition {
    column: u16,
    row: u16,
}

impl CursorPosition {
    fn new(column: u16, row: u16) -> Self {
        Self { column, row }
    }
}

/// The different movements that can can be handled in `Navigation` mode.
#[derive(Debug)]
enum CursorMovement {
    Left,
    Right,
    Up,
    Down,
}

/// The core class of the application.
/// This provides both the text buffer and the rendering of the buffer in the terminal.
/// These functions should really be separated at some point, but it was quick to implement in this fashion.
pub struct Editor {
    /// The path to the file that this buffer should be written into.
    path: Option<PathBuf>,
    /// The contents of the buffer.
    contents: String,
    /// The current position of the cursor on the display.
    cursor: CursorPosition,
    /// The size of the display.
    display_size: DisplaySize,
    /// The position of the buffer in the display.
    display_position: DisplayPosition,
    /// The current mode that the editor is in.
    mode: EditorMode,
}

impl Editor {
    /// Creates a new `Editor` instance with the supplied string copied into its buffer.
    pub fn new(s: &str) -> Self {
        Self {
            path: None,
            contents: s.to_string(),
            cursor: Default::default(),
            display_size: Default::default(),
            display_position: Default::default(),
            mode: Default::default(),
        }
    }

    /// Wraps a new `Editor` instance around a path on the filesystem.
    ///
    /// If the file exists it will be read into the buffer, if it does not then an empty buffer will be initialised.
    /// The directory that the file is in must exist, because we don't currently have a way of displaying the error we would receive when we try to write the file.
    pub fn from_path<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let mut contents = String::new();
        let mut file = PathBuf::from("./");
        // If `path` is absolute then this will flush out the `"./"` currently in the buffer, which is pretty neat!
        file.push(&path);

        match file.parent().map(|d| d.exists()) {
            Some(true) => (),
            Some(false) => {
                return Err(EditorError::DirectoryDoesNotExist(
                    file.parent().unwrap().to_path_buf(),
                ))
            }
            None => return Err(EditorError::CannotOpenRoot),
        }

        if file.exists() {
            let mut file = File::open(&path).map_err(|e| EditorError::FileIo(e))?;
            file.read_to_string(&mut contents)
                .map_err(|e| EditorError::FileIo(e))?;
        }

        Ok(Self::new(&contents))
    }

    /// Determines the length of the row the cursor currently sits on.
    fn row_length(&self) -> usize {
        self.contents
            .lines()
            .nth(self.display_position.row + self.cursor.row as usize)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Determines the position of the cursor in the `contents` buffer.
    fn cursor_index(&self) -> usize {
        self.contents
            .lines()
            .take(self.cursor.row as usize + self.display_position.row)
            .map(|s| s.len() + 1)
            .sum::<usize>()
            + self.cursor.column as usize
    }

    fn move_cursor(&mut self, direction: CursorMovement) {
        match direction {
            CursorMovement::Left => match self.cursor.column.checked_sub(1) {
                Some(c) => self.cursor.column = c,
                None => {
                    self.display_position.column = self.display_position.column.saturating_sub(1)
                }
            },
            CursorMovement::Right => {
                let last_column = match self.mode {
                    EditorMode::Edit => self.row_length(),
                    _ => self.row_length().saturating_sub(1),
                };

                let can_move_right =
                    self.display_position.column + (self.cursor.column as usize) < last_column;
                let at_right_of_display = self.cursor.column == self.display_size.columns - 1;

                match (can_move_right, at_right_of_display) {
                    (true, true) => self.display_position.column = self.display_position.column + 1,
                    (true, _) => self.cursor.column = self.cursor.column + 1,
                    (_, _) => (),
                }
            }
            CursorMovement::Up => {
                match self.cursor.row.checked_sub(1) {
                    Some(r) => self.cursor.row = r,
                    None => self.display_position.row = self.display_position.row.saturating_sub(1),
                }

                self.display_position.column =
                    cmp::min(self.display_position.column, self.row_length());

                self.cursor.column = cmp::min(
                    self.cursor.column,
                    (self.row_length() - self.display_position.column)
                        .try_into()
                        .unwrap_or(u16::MAX),
                );
            }
            CursorMovement::Down => {
                let num_lines = self
                    .contents
                    .lines()
                    .skip(self.display_position.row)
                    .count();
                let can_move_down = self.cursor.row + 1 < num_lines.try_into().unwrap_or(u16::MAX);
                let at_bottom_of_display = self.display_size.rows == (self.cursor.row + 1).into();

                match (can_move_down, at_bottom_of_display) {
                    (true, true) => self.display_position.row = self.display_position.row + 1,
                    (true, _) => self.cursor.row = self.cursor.row + 1,
                    (_, _) => (),
                }

                self.display_position.column =
                    cmp::min(self.display_position.column, self.row_length());

                self.cursor.column = cmp::min(
                    self.cursor.column,
                    (self.row_length() - self.display_position.column)
                        .try_into()
                        .unwrap_or(u16::MAX),
                );
            }
        }
    }

    /// Inserts a character into the `contents` buffer at the cursor position.
    fn insert(&mut self, c: char) {
        self.contents.insert(self.cursor_index(), c);

        if c == '\n' {
            self.cursor.column = 0;
            self.cursor.row = self.cursor.row + 1;
        } else {
            self.cursor.column = self.cursor.column + 1;
        }
    }

    /// Removes a character from the `contents` buffer at the cursor position.
    fn remove(&mut self) {
        if let Some(idx) = self.cursor_index().checked_sub(1) {
            let current_length = self.row_length();
            match self.contents.remove(idx) {
                '\n' => {
                    self.cursor.row = self.cursor.row - 1;
                    self.cursor.column = (self.row_length() - current_length)
                        .try_into()
                        .unwrap_or(u16::MAX);
                }
                _ => self.cursor.column = self.cursor.column - 1,
            }
        }
    }

    pub fn set_display_columns(&mut self, c: u16) {
        self.display_size.columns = c;
        self.display_position.column = cmp::min(self.display_position.column, c.into());
    }

    pub fn set_display_rows(&mut self, r: u16) {
        self.display_size.rows = r;
        self.display_position.row = cmp::min(self.display_position.row, r.into());
    }

    /// Renders the editor to a stream, assuming that a TTY is on the other end.
    fn render<S: Write + IsTty>(&mut self, stream: &mut S) -> Result<()> {
        queue!(
            stream,
            cursor::Hide,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .map_err(|e| EditorError::TermIo(e))?;

        for (row, line) in self
            .contents
            .lines()
            .skip(self.display_position.row)
            .take(self.display_size.rows.into())
            .enumerate()
        {
            queue!(stream, cursor::MoveTo(0, row.try_into().unwrap()))
                .map_err(|e| EditorError::TermIo(e))?;
            match line.get(self.display_position.column..) {
                Some(s) => match s.get(..self.display_size.columns.into()) {
                    Some(s2) => write!(stream, "{}", s2).map_err(|e| EditorError::TermIo(e))?,
                    None => write!(stream, "{}", s).map_err(|e| EditorError::TermIo(e))?,
                },
                None => (),
            }
        }

        let mut last_column = self.row_length();
        match self.mode {
            EditorMode::Edit => (),
            _ => last_column = last_column.saturating_sub(1),
        }

        queue!(
            stream,
            cursor::MoveTo(
                cmp::min(self.cursor.column as usize, last_column)
                    .try_into()
                    .unwrap(),
                self.cursor.row
            ),
            cursor::Show
        )
        .map_err(|e| EditorError::TermIo(e))?;

        stream.flush().map_err(|e| EditorError::TermIo(e))
    }

    /// Writes the `contents` buffer to the file at `path`.
    fn write(&self) -> Result<()> {
        let mut file =
            File::create(self.path.as_ref().unwrap()).map_err(|e| EditorError::FileIo(e))?;
        file.write(self.contents.as_bytes())
            .map_err(|e| EditorError::FileIo(e))?;
        Ok(())
    }

    /// Runs the `Editor`'s main loop.
    pub fn run<T>(&mut self, stream: &mut T) -> Result<()>
    where
        T: Write + IsTty,
    {
        loop {
            self.render(stream)?;

            match self.mode {
                EditorMode::Navigate => match event::read().map_err(|e| EditorError::TermIo(e))? {
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('q'),
                        ..
                    }) => break,
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('h'),
                        ..
                    }) => self.move_cursor(CursorMovement::Left),
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('j'),
                        ..
                    }) => self.move_cursor(CursorMovement::Down),
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('k'),
                        ..
                    }) => self.move_cursor(CursorMovement::Up),
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('l'),
                        ..
                    }) => self.move_cursor(CursorMovement::Right),
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('i'),
                        ..
                    }) => self.mode = EditorMode::Edit,
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('w'),
                        ..
                    }) => self.write()?,
                    _ => (),
                },
                EditorMode::Edit => match event::read().map_err(|e| EditorError::TermIo(e))? {
                    Event::Key(KeyEvent {
                        code: KeyCode::Esc, ..
                    }) => self.mode = EditorMode::Navigate,
                    Event::Key(KeyEvent {
                        code: KeyCode::Char(c),
                        ..
                    }) => self.insert(c),
                    Event::Key(KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    }) => self.insert('\n'),
                    Event::Key(KeyEvent {
                        code: KeyCode::Backspace,
                        ..
                    }) => self.remove(),
                    _ => (),
                },
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_moving_cursor_left() {
        let mut editor = Editor::new("...\n...\n...");
        editor.cursor = CursorPosition::new(1, 1);
        editor.move_cursor(CursorMovement::Left);
        assert_eq!(editor.cursor, CursorPosition::new(0, 1));
    }

    #[test]
    fn test_moving_cursor_right() {
        let mut editor = Editor::new("...\n...\n...");
        editor.cursor = CursorPosition::new(1, 1);
        editor.move_cursor(CursorMovement::Right);
        assert_eq!(editor.cursor, CursorPosition::new(2, 1));
    }

    #[test]
    fn test_moving_cursor_up() {
        let mut editor = Editor::new("...\n...\n...");
        editor.cursor = CursorPosition::new(1, 1);
        editor.move_cursor(CursorMovement::Up);
        assert_eq!(editor.cursor, CursorPosition::new(1, 0));
    }

    #[test]
    fn test_moving_cursor_down() {
        let mut editor = Editor::new("...\n...\n...");
        editor.cursor = CursorPosition::new(1, 1);
        editor.move_cursor(CursorMovement::Down);
        assert_eq!(editor.cursor, CursorPosition::new(1, 2));
    }

    #[test]
    fn test_moving_cursor_left_at_edge() {
        let mut editor = Editor::new("...\n...\n...");
        editor.cursor = CursorPosition::new(0, 1);
        editor.move_cursor(CursorMovement::Left);
        assert_eq!(editor.cursor, CursorPosition::new(0, 1));
    }

    #[test]
    fn test_moving_cursor_right_at_edge() {
        let mut editor = Editor::new("...\n...\n...");
        editor.cursor = CursorPosition::new(2, 1);
        editor.move_cursor(CursorMovement::Right);
        assert_eq!(editor.cursor, CursorPosition::new(2, 1));
    }

    #[test]
    fn test_moving_cursor_up_at_edge() {
        let mut editor = Editor::new("...\n...\n...");
        editor.cursor = CursorPosition::new(1, 0);
        editor.move_cursor(CursorMovement::Up);
        assert_eq!(editor.cursor, CursorPosition::new(1, 0));
    }

    #[test]
    fn test_moving_cursor_down_at_edge() {
        let mut editor = Editor::new("...\n...\n...");
        editor.cursor = CursorPosition::new(1, 2);
        editor.move_cursor(CursorMovement::Down);
        assert_eq!(editor.cursor, CursorPosition::new(1, 2));
    }

    #[test]
    fn test_scrolling_down() {
        let mut editor = Editor::new("1\n2\n3\n4\n5");
        editor.set_display_rows(3);
        editor.cursor = CursorPosition::new(0, 2);
        editor.display_position = DisplayPosition::new(0, 0);

        editor.move_cursor(CursorMovement::Down);

        assert_eq!(editor.cursor, CursorPosition::new(0, 2));
        assert_eq!(editor.display_position, DisplayPosition::new(0, 1));
    }

    #[test]
    fn test_scrolling_right() {
        let mut editor = Editor::new("12345");
        editor.set_display_columns(3);
        editor.cursor = CursorPosition::new(2, 0);
        editor.display_position = DisplayPosition::new(0, 0);

        editor.move_cursor(CursorMovement::Right);

        assert_eq!(editor.cursor, CursorPosition::new(2, 0));
        assert_eq!(editor.display_position, DisplayPosition::new(1, 0));
    }

    #[test]
    fn test_scrolling_left() {
        let mut editor = Editor::new("12345");
        editor.cursor = CursorPosition::new(0, 0);
        editor.display_position = DisplayPosition::new(1, 0);

        editor.move_cursor(CursorMovement::Left);

        assert_eq!(editor.cursor, CursorPosition::new(0, 0));
        assert_eq!(editor.display_position, DisplayPosition::new(0, 0));
    }

    #[test]
    fn test_scrolling_up() {
        let mut editor = Editor::new("1\n2\n3\n4\n5");
        editor.cursor = CursorPosition::new(0, 0);
        editor.display_position = DisplayPosition::new(0, 2);

        editor.move_cursor(CursorMovement::Up);

        assert_eq!(editor.cursor, CursorPosition::new(0, 0));
        assert_eq!(editor.display_position, DisplayPosition::new(0, 1));
    }

    #[test]
    fn test_scrolling_down_at_end() {
        let mut editor = Editor::new("1\n2\n3\n4\n5");
        editor.cursor = CursorPosition::new(0, 2);
        editor.display_position.row = 2;
        editor.move_cursor(CursorMovement::Down);
        assert_eq!(editor.cursor, CursorPosition::new(0, 2));
        assert_eq!(editor.display_position, DisplayPosition::new(0, 2));
    }

    #[test]
    fn test_scrolling_up_at_end() {
        let mut editor = Editor::new("1\n2\n3\n4\n5");
        editor.cursor = CursorPosition::new(0, 0);
        editor.move_cursor(CursorMovement::Up);
        assert_eq!(editor.cursor, CursorPosition::new(0, 0));
        assert_eq!(editor.display_position, DisplayPosition::new(0, 0));
    }

    #[test]
    fn test_inserting_a_char() {
        let mut editor = Editor::new("");
        editor.insert('a');
        assert_eq!(editor.contents, "a");
    }

    #[test]
    fn test_inserting_multiple_chars() {
        let mut editor = Editor::new("");
        editor.insert('a');
        editor.insert('b');
        editor.insert('c');
        assert_eq!(editor.contents, "abc");
    }

    #[test]
    fn test_inserting_multiple_lines() {
        let mut editor = Editor::new("");
        editor.insert('a');
        editor.insert('\n');
        editor.insert('b');
        editor.insert('\n');
        editor.insert('c');
        assert_eq!(editor.contents, "a\nb\nc");
    }

    #[test]
    fn test_removing_a_char() {
        let mut editor = Editor::new("abc");
        editor.cursor = CursorPosition::new(2, 0);
        editor.remove();
        assert_eq!(editor.contents, "ac");
    }

    #[test]
    fn test_removing_from_empty_buffer() {
        let mut editor = Editor::new("");
        editor.remove();
        assert_eq!(editor.contents, "");
    }
}
