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
            return Err(io::Error::other("Download exceeds the file size limit."));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
