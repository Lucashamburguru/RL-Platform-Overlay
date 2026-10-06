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
    ctx.set_fonts(fallback_definitions(&paths));
}

fn fallback_definitions(paths: &[std::path::PathBuf]) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    for (index, path) in paths.iter().enumerate() {
        if std::fs::metadata(path).is_ok_and(|metadata| metadata.len() <= 32 * 1024 * 1024)
            && let Ok(bytes) = std::fs::read(path)
        {
            if ab_glyph::FontRef::try_from_slice(&bytes).is_err() {
                log::warn!("Skipping invalid system fallback font: {}", path.display());
                continue;
            }
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
    fonts
}

#[cfg(test)]
mod tests {
    #[test]
    fn invalid_font_is_rejected_before_egui_can_panic() {
        let dir = tempfile::tempdir().unwrap();
        let corrupt = dir.path().join("corrupt.ttf");
        let valid = dir.path().join("valid.ttf");
        std::fs::write(&corrupt, b"corrupt font").unwrap();
        let defaults = eframe::egui::FontDefinitions::default();
        std::fs::write(
            &valid,
            defaults.font_data.values().next().unwrap().font.as_ref(),
        )
        .unwrap();
        let fonts = super::fallback_definitions(&[corrupt, valid]);
        assert!(!fonts.font_data.contains_key("system_fallback_0"));
        assert!(fonts.font_data.contains_key("system_fallback_1"));
        let ctx = eframe::egui::Context::default();
        ctx.set_fonts(fonts);
        let output = ctx.run(Default::default(), |ctx| {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("Overlay starts with a corrupt installed font");
            });
        });
        assert!(!output.shapes.is_empty());
    }
}
