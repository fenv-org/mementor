use mementor_lib::context::RealMementorContext;
use mementor_lib::output::BufferedIO;

/// Run a CLI test with a temporary project directory and buffered I/O.
pub fn test_with_context<F>(f: F)
where
    F: FnOnce(&RealMementorContext, &mut BufferedIO),
{
    let tmp = tempfile::tempdir().unwrap();
    let context = RealMementorContext::new(tmp.path().to_path_buf());
    let mut io = BufferedIO::new();
    f(&context, &mut io);
}
