use std::io::{self, Write};

pub struct LimitedWriter<W> {
    inner: W,
    remaining: u64,
}

impl<W: Write> LimitedWriter<W> {
    pub fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other("Output exceeds the byte limit."));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub(crate) fn bounded_text(bytes: &[u8], limit: usize) -> io::Result<String> {
    let text = String::from_utf8_lossy(bytes);
    if text.len() > limit {
        return Err(io::Error::other(
            "Decoded output exceeds the text byte limit.",
        ));
    }
    Ok(text.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_characters_do_not_expand_retained_text_past_byte_limit() {
        assert!(bounded_text(&[0xff; 4], 4).is_err());
        assert_eq!(bounded_text("é😀".as_bytes(), 6).unwrap(), "é😀");
        assert!(bounded_text("é😀".as_bytes(), 5).is_err());
    }

    #[test]
    fn rejects_growth_without_writing_past_the_limit() {
        let mut bytes = Vec::new();
        let mut writer = LimitedWriter::new(&mut bytes, 4);
        writer.write_all(b"abc").unwrap();
        assert!(writer.write_all(b"de").is_err());
        writer.write_all(b"d").unwrap();
        assert!(writer.write_all(b"e").is_err());
        assert_eq!(bytes, b"abcd");
    }

    #[test]
    fn preserves_destination_errors() {
        struct FullDisk;
        impl Write for FullDisk {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "read only"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut writer = LimitedWriter::new(FullDisk, 4);
        assert_eq!(
            writer.write(b"a").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(writer.remaining, 4);
    }
}
