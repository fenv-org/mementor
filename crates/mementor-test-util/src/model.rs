use std::path::PathBuf;

/// Return the model cache directory for tests.
///
/// Checks `MEMENTOR_MODEL_DIR` first, falling back to `~/.mementor/models/`.
pub fn model_dir() -> PathBuf {
    std::env::var("MEMENTOR_MODEL_DIR").map_or_else(
        |_| {
            dirs::home_dir()
                .expect("home dir")
                .join(".mementor")
                .join("models")
        },
        PathBuf::from,
    )
}
