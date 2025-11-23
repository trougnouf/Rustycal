use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    cfait::tui::run().await
}
