use eframe::egui;

/// Keep egui's normal fonts first and fill missing glyphs from installed fonts.
/// Called once at startup, never from a render callback.
pub(crate) fn install_fallbacks(ctx: &egui::Context) {
    let mut paths = Vec::new();
    #[cfg(target_os = "linux")]
    for pattern in [":lang=zh-cn", ":lang=ja", ":lang=ko", ":charset=2600"] {
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", pattern])
            .output()
            && output.status.success()
        {
            paths.push(std::path::PathBuf::from(
                String::from_utf8_lossy(&output.stdout).trim(),
            ));
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(windows) = std::env::var_os("WINDIR") {
        let directory = std::path::PathBuf::from(windows).join("Fonts");
        for name in ["seguisym.ttf", "msyh.ttc", "meiryo.ttc", "malgun.ttf"] {
            paths.push(directory.join(name));
        }
    }
    #[cfg(target_os = "macos")]
    paths.push(std::path::PathBuf::from(
        "/System/Library/Fonts/PingFang.ttc",
    ));
    paths.sort();
    paths.dedup();
    let mut fonts = egui::FontDefinitions::default();
    for (index, path) in paths.iter().enumerate() {
        if std::fs::metadata(path).is_ok_and(|metadata| metadata.len() <= 32 * 1024 * 1024)
            && let Ok(bytes) = std::fs::read(path)
        {
            let name = format!("system_fallback_{index}");
            fonts.font_data.insert(
                name.clone(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push(name.clone());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push(name);
        }
    }
    ctx.set_fonts(fonts);
}
