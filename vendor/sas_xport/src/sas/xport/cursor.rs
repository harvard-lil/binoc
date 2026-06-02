#[derive(Debug)]
pub(crate) struct Cursor<'a> {
    buffer: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    pub fn position(&mut self, position: usize) {
        self.position = position;
    }

    pub fn read(&mut self, length: usize) -> &'a [u8] {
        let end = self.position + length;
        let read = &self.buffer[self.position..end];
        self.position = end;
        read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_position_zero() {
        let cursor = Cursor::new(b"ABCDEF");
        assert_eq!(0, cursor.position);
    }

    #[test]
    fn read_returns_slice_and_advances() {
        let mut cursor = Cursor::new(b"ABCDEF");
        assert_eq!(b"AB", cursor.read(2));
        assert_eq!(2, cursor.position);
    }

    #[test]
    fn sequential_reads_return_consecutive_slices() {
        let mut cursor = Cursor::new(b"ABCDEF");
        assert_eq!(b"AB", cursor.read(2));
        assert_eq!(2, cursor.position);
        assert_eq!(b"CD", cursor.read(2));
        assert_eq!(4, cursor.position);
        assert_eq!(b"EF", cursor.read(2));
        assert_eq!(6, cursor.position);
    }

    #[test]
    fn read_entire_buffer() {
        let mut cursor = Cursor::new(b"ABCD");
        assert_eq!(b"ABCD", cursor.read(4));
        assert_eq!(4, cursor.position);
    }

    #[test]
    fn read_zero_returns_empty_slice() {
        let mut cursor = Cursor::new(b"ABCD");
        assert_eq!(b"", cursor.read(0));
        assert_eq!(0, cursor.position);
    }

    #[test]
    fn set_position_changes_read_offset() {
        let mut cursor = Cursor::new(b"ABCDEF");
        cursor.position(4);
        assert_eq!(4, cursor.position);
        assert_eq!(b"EF", cursor.read(2));
        assert_eq!(6, cursor.position);
    }

    #[test]
    fn set_position_can_rewind() {
        let mut cursor = Cursor::new(b"ABCDEF");
        cursor.read(4);
        assert_eq!(4, cursor.position);
        cursor.position(0);
        assert_eq!(0, cursor.position);
        assert_eq!(b"AB", cursor.read(2));
        assert_eq!(2, cursor.position);
    }
}
