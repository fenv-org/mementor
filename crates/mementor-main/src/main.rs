use std::path::Path;

use mementor_lib::cache::DataCache;
use mementor_lib::git::branch::current_branch;
use mementor_lib::git::resolve_worktree;
use mementor_tui::app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cwd = std::env::current_dir()?;
    let resolved = resolve_worktree(&cwd);
    let _project_root = resolved
        .primary_root()
        .map_or_else(|| cwd.clone(), Path::to_path_buf);

    let branch = current_branch().await.unwrap_or_else(|_| "main".into());
    let cache = DataCache::initialize(&branch).await?;

    let mut terminal = App::setup_terminal()?;
    let mut app = App::new(cache, branch);

    let result = app.run(&mut terminal).await;

    App::restore_terminal(&mut terminal)?;

    result
}
