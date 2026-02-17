use std::path::PathBuf;

use crate::context::RealMementorContext;
use crate::output::BufferedIO;

/// Run a test with a temporary project directory and buffered I/O.
pub fn test_with_context<F>(f: F)
where
    F: FnOnce(&RealMementorContext, &mut BufferedIO),
{
    let tmp = tempfile::tempdir().unwrap();
    let context = RealMementorContext::new(tmp.path().to_path_buf());
    let mut io = BufferedIO::new();
    f(&context, &mut io);
}

/// Run a test and return the temporary directory alongside results
/// (useful when the test needs the temp dir to persist for assertions).
pub fn test_with_context_and_dir<F>(f: F) -> (tempfile::TempDir, String, String)
where
    F: FnOnce(&RealMementorContext, &mut BufferedIO),
{
    let tmp = tempfile::tempdir().unwrap();
    let context = RealMementorContext::new(tmp.path().to_path_buf());
    let mut io = BufferedIO::new();
    f(&context, &mut io);
    let stdout = io.stdout_to_string();
    let stderr = io.stderr_to_string();
    (tmp, stdout, stderr)
}

/// Create a `RealMementorContext` from an explicit path.
pub fn context_at(path: PathBuf) -> RealMementorContext {
    RealMementorContext::new(path)
}
