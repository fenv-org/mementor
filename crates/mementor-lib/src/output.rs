use std::io::{Cursor, Read, Stderr, Stdin, Stdout, Write};

/// Abstracts stdin/stdout/stderr for dependency injection and testability.
pub trait ConsoleIO<IN: Read, OUT: Write, ERR: Write> {
    fn stdin(&mut self) -> &mut IN;
    fn stdout(&mut self) -> &mut OUT;
    fn stderr(&mut self) -> &mut ERR;
}

/// Real implementation that uses actual stdin/stdout/stderr.
pub struct StdIO {
    stdin: Stdin,
    stdout: Stdout,
    stderr: Stderr,
}

impl StdIO {
    #[must_use]
    pub fn new() -> Self {
        Self {
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
            stderr: std::io::stderr(),
        }
    }
}

impl Default for StdIO {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleIO<Stdin, Stdout, Stderr> for StdIO {
    fn stdin(&mut self) -> &mut Stdin {
        &mut self.stdin
    }

    fn stdout(&mut self) -> &mut Stdout {
        &mut self.stdout
    }

    fn stderr(&mut self) -> &mut Stderr {
        &mut self.stderr
    }
}

/// Test implementation that captures output and provides canned stdin.
pub struct BufferedIO {
    stdin: Cursor<Vec<u8>>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl BufferedIO {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `BufferedIO` with pre-filled stdin data.
    #[must_use]
    pub fn with_stdin(data: &[u8]) -> Self {
        Self {
            stdin: Cursor::new(data.to_vec()),
            ..Self::default()
        }
    }

    /// Returns the captured stdout content as a string.
    pub fn stdout_to_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Returns the captured stderr content as a string.
    pub fn stderr_to_string(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

impl Default for BufferedIO {
    fn default() -> Self {
        Self {
            stdin: Cursor::new(Vec::new()),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
}

impl ConsoleIO<Cursor<Vec<u8>>, Vec<u8>, Vec<u8>> for BufferedIO {
    fn stdin(&mut self) -> &mut Cursor<Vec<u8>> {
        &mut self.stdin
    }

    fn stdout(&mut self) -> &mut Vec<u8> {
        &mut self.stdout
    }

    fn stderr(&mut self) -> &mut Vec<u8> {
        &mut self.stderr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffered_io_captures_stdout() {
        let mut io = BufferedIO::new();
        writeln!(io.stdout(), "hello").unwrap();
        assert_eq!(io.stdout_to_string(), "hello\n");
    }

    #[test]
    fn buffered_io_captures_stderr() {
        let mut io = BufferedIO::new();
        writeln!(io.stderr(), "error").unwrap();
        assert_eq!(io.stderr_to_string(), "error\n");
    }

    #[test]
    fn buffered_io_starts_empty() {
        let io = BufferedIO::new();
        assert!(io.stdout_to_string().is_empty());
        assert!(io.stderr_to_string().is_empty());
    }

    #[test]
    fn buffered_io_reads_stdin() {
        let mut io = BufferedIO::with_stdin(b"hello stdin");
        let mut buf = String::new();
        io.stdin().read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "hello stdin");
    }
}
