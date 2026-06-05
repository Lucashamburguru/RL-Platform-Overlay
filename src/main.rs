#[tokio::main]
async fn main() -> eframe::Result<()> {
    let debug_enabled = std::env::args().any(|arg| arg == "--debug");
    rl_platform_overlay::run(debug_enabled).await
}
