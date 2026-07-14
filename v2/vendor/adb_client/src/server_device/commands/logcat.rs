use std::io::{self, Write};

use crate::{ADBDeviceExt, Result, server_device::ADBServerDevice};

struct LogFilter<W: Write> {
    writer: W,
    buffer: Vec<u8>,
}

impl<W: Write> LogFilter<W> {
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            buffer: Vec::new(),
        }
    }
}

impl<W: Write> Write for LogFilter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Add newly received bytes to the internal buffer
        self.buffer.extend_from_slice(buf);

        let mut processed = 0;
        while let Some(pos) = self.buffer[processed..].iter().position(|&b| b == b'\n') {
            // Found a newline, need to process it
            let end = processed + pos + 1; // +1 to include the '\n'
            let line = &self.buffer[processed..end];

            self.writer.write_all(line)?;

            processed = end;
        }

        // Keep only remaining bytes after the last complete line
        if processed > 0 {
            self.buffer.copy_within(processed.., 0);
            self.buffer.truncate(self.buffer.len() - processed);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl ADBServerDevice {
    /// Get logs from device
    pub fn get_logs<W: Write>(&mut self, output: W) -> Result<()> {
        let _status = self.shell_command(&"exec logcat", Some(&mut LogFilter::new(output)), None);
        Ok(())
    }
}
