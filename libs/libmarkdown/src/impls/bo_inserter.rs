use std::io::{self, Write};

pub struct BoInserter<W> {
    w: W,
    byte_offset: usize,
    state: State,
}

impl<W> BoInserter<W>
where
    W: Write,
{
    pub fn new(w: W, byte_offset: usize) -> Self {
        BoInserter {
            w,
            byte_offset,
            state: State::LookingForOpeningChevron,
        }
    }
}

enum State {
    LookingForOpeningChevron,
    LookingForClosingChevron,
    InsertingDataBo,
    PassingThrough,
}

impl<W> Write for BoInserter<W>
where
    W: Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0;

        for &byte in buf {
            match self.state {
                State::LookingForOpeningChevron => {
                    if byte == b'<' {
                        self.state = State::LookingForClosingChevron;
                    }
                }
                State::LookingForClosingChevron => {
                    if byte == b' ' || byte == b'>' {
                        let bo_str = format!(" data-bo=\"{}\"", self.byte_offset);
                        self.w.write_all(bo_str.as_bytes())?;
                        self.state = State::InsertingDataBo;
                    }
                }
                State::InsertingDataBo => {
                    if byte == b'>' {
                        self.state = State::PassingThrough;
                    }
                }
                State::PassingThrough => {}
            }

            self.w.write_all(&[byte])?;
            written += 1;
        }

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_insert_data_bo() {
        let mut cursor = Cursor::new(Vec::new());
        let mut inserter = BoInserter::new(&mut cursor, 123);

        // Write a simple HTML tag
        inserter.write_all(b"<div>").unwrap();

        // Check the output
        let result = String::from_utf8(cursor.into_inner()).unwrap();
        assert_eq!(result, "<div data-bo=\"123\">");
    }

    #[test]
    fn test_pass_through() {
        let mut cursor = Cursor::new(Vec::new());
        let mut inserter = BoInserter::new(&mut cursor, 123);

        // Write some text that should pass through unchanged
        inserter.write_all(b"Some text").unwrap();

        // Check the output
        let result = String::from_utf8(cursor.into_inner()).unwrap();
        assert_eq!(result, "Some text");
    }

    #[test]
    fn test_multiple_tags() {
        let mut cursor = Cursor::new(Vec::new());
        let mut inserter = BoInserter::new(&mut cursor, 123);

        // Write multiple HTML tags
        inserter.write_all(b"<div><span>").unwrap();

        // Check the output
        let result = String::from_utf8(cursor.into_inner()).unwrap();
        assert_eq!(result, "<div data-bo=\"123\"><span>");
    }

    #[test]
    fn test_nested_tags() {
        let mut cursor = Cursor::new(Vec::new());
        let mut inserter = BoInserter::new(&mut cursor, 123);

        // Write nested HTML tags
        inserter.write_all(b"<div><span><p>").unwrap();

        // Check the output
        let result = String::from_utf8(cursor.into_inner()).unwrap();
        assert_eq!(result, "<div data-bo=\"123\"><span><p>");
    }

    #[test]
    fn test_flush() {
        let mut cursor = Cursor::new(Vec::new());
        let mut inserter = BoInserter::new(&mut cursor, 123);

        // Write some text
        inserter.write_all(b"Some text").unwrap();

        // Flush the inserter
        inserter.flush().unwrap();

        // Check the output
        let result = String::from_utf8(cursor.into_inner()).unwrap();
        assert_eq!(result, "Some text");
    }
}
