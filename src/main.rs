fn main() -> Result<(), Box<dyn std::error::Error>> {
    let debug_enabled = std::env::args().any(|arg| arg == "--debug");
    rl_platform_overlay::crash_logging::init(debug_enabled);
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            log::error!("Could not create async runtime: {error}");
            return Err(error.into());
        }
    };
    runtime.block_on(rl_platform_overlay::run(debug_enabled))?;
    log::info!("Session completed normally.");
    Ok(())
}
