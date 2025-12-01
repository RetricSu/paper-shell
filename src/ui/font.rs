/// Font setup and configuration for the application
///
/// Handles system font loading with CJK (Chinese, Japanese, Korean) support
use eframe::egui::{FontData, FontDefinitions, FontFamily};

/// Setup fonts for the application with CJK support
///
/// This function attempts to load system fonts with CJK support based on the current OS.
/// If no suitable CJK font is found, it falls back to a generic sans-serif font.
///
/// # Font Priority by OS:
/// - macOS: PingFang SC, Hiragino Sans GB, STSong, Heiti SC
/// - Windows: Microsoft YaHei, SimSun, SimHei, MS Gothic
/// - Linux: Noto Sans CJK TC
///
/// # Returns
/// A `FontDefinitions` instance configured with the best available system font
pub fn setup_fonts() -> FontDefinitions {
    // Create font definitions - start with defaults so we have fallbacks
    let mut fonts = FontDefinitions::default();

    // Try to find a system font with CJK support
    let source = font_kit::source::SystemSource::new();

    // Define font names to try based on OS for better CJK support
    let font_names: Vec<&str> = get_preferred_font_names();

    // Try to find one of the preferred fonts
    let mut found_font = false;
    for font_name in font_names {
        if try_load_font(&mut fonts, &source, font_name) {
            tracing::info!("Using system font '{}' for CJK support", font_name);
            found_font = true;
            break;
        }
    }

    // If we couldn't find any preferred fonts, try a generic sans-serif as backup
    if !found_font {
        load_fallback_font(&mut fonts, &source);
    }

    fonts
}

/// Get preferred font names based on the current operating system
fn get_preferred_font_names() -> Vec<&'static str> {
    match std::env::consts::OS {
        "macos" => vec!["Hiragino Sans GB", "PingFang SC", "Heiti SC", "STSong"],
        "windows" => vec!["Microsoft YaHei", "SimSun", "SimHei", "MS Gothic"],
        "linux" => vec!["Noto Sans CJK TC"],
        _ => vec![], // Empty for other OSes - we'll use generic fallback
    }
}

/// Try to load a specific font by name
///
/// # Returns
/// `true` if the font was successfully loaded and registered, `false` otherwise
fn try_load_font(
    fonts: &mut FontDefinitions,
    source: &font_kit::source::SystemSource,
    font_name: &str,
) -> bool {
    // Get family by name
    if let Ok(family_handle) = source.select_family_by_name(font_name)
        && let Some(font_handle) = family_handle.fonts().first()
        && let Ok(font_data) = match font_handle {
            font_kit::handle::Handle::Memory { bytes, .. } => Ok(bytes.to_vec()),
            font_kit::handle::Handle::Path { path, .. } => std::fs::read(path),
        }
    {
        // Register the font with egui
        const SYSTEM_FONT_NAME: &str = "SystemCJKFont";
        fonts.font_data.insert(
            SYSTEM_FONT_NAME.to_owned(),
            FontData::from_owned(font_data)
                .tweak(eframe::egui::FontTweak {
                    y_offset_factor: 0.3, // Adjust this value to fix vertical alignment (e.g. -0.2 or 0.2)
                    ..Default::default()
                })
                .into(),
        );

        // Add as primary font for proportional text (at the beginning)
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, SYSTEM_FONT_NAME.to_owned());

        // Also add to monospace as a fallback
        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .push(SYSTEM_FONT_NAME.to_owned());

        return true;
    }

    false
}

/// Load a fallback font (generic sans-serif) when no preferred font is available
fn load_fallback_font(fonts: &mut FontDefinitions, source: &font_kit::source::SystemSource) {
    if let Ok(font_handle) = source.select_best_match(
        &[font_kit::family_name::FamilyName::SansSerif],
        &font_kit::properties::Properties::new(),
    ) {
        if let Ok(font_data) = match font_handle {
            font_kit::handle::Handle::Memory { bytes, .. } => Ok(bytes.to_vec()),
            font_kit::handle::Handle::Path { path, .. } => std::fs::read(&path),
        } {
            const SYSTEM_FONT_NAME: &str = "SystemFont";
            fonts.font_data.insert(
                SYSTEM_FONT_NAME.to_owned(),
                FontData::from_owned(font_data)
                    .tweak(eframe::egui::FontTweak {
                        y_offset_factor: 0.0, // Adjust this value to fix vertical alignment (e.g. -0.2 or 0.2)
                        ..Default::default()
                    })
                    .into(),
            );

            // Add as primary font
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, SYSTEM_FONT_NAME.to_owned());

            tracing::info!("Using generic system font for text");
        } else {
            tracing::warn!("Could not load system font data, using defaults");
        }
    } else {
        tracing::warn!("Could not find suitable system font, using defaults");
    }
}

/// Enumerate all available Chinese fonts from the system
///
/// This function scans the system for fonts and returns a list of font family names
/// that have CJK (Chinese, Japanese, Korean) support. The detection is based on:
/// - Font family name patterns (common Chinese font names)
/// - Operating system defaults
///
/// # Returns
/// A sorted vector of unique font family names that support Chinese characters
pub fn enumerate_chinese_fonts() -> Vec<String> {
    let source = font_kit::source::SystemSource::new();
    let mut chinese_fonts = std::collections::HashSet::new();

    // Get all font families
    let families = source.all_families().unwrap_or_default();

    for family_name in families {
        // Check if the font name contains common Chinese font indicators
        if is_likely_chinese_font(&family_name) {
            chinese_fonts.insert(family_name);
        }
    }

    // Also include our known preferred fonts for the current OS
    for font_name in get_preferred_font_names() {
        chinese_fonts.insert(font_name.to_string());
    }

    // Convert to sorted vector for consistent ordering
    let mut result: Vec<String> = chinese_fonts.into_iter().collect();
    result.sort();

    tracing::info!("Found {} Chinese fonts on the system", result.len());
    result
}

/// Check if a font name is likely to be a Chinese font
///
/// This checks for common patterns in Chinese font names across different platforms
fn is_likely_chinese_font(name: &str) -> bool {
    let name_lower = name.to_lowercase();

    // Common Chinese font name patterns
    let chinese_indicators = [
        // Simplified Chinese
        "pingfang",
        "hiragino",
        "heiti",
        "stheiti",
        "stsong",
        "stkaiti",
        "stfangsong",
        "songti",
        "kaiti",
        "fangsong",
        "yahei",
        "microsoft yahei",
        "simsun",
        "simhei",
        "simkai",
        "nsimsun",
        "fangsong",
        "lishu",
        "deng",
        "yuan",
        // Traditional Chinese
        "lihei",
        "lisung",
        "pmingliu",
        "mingliu",
        // Japanese (often have CJK support)
        "gothic",
        "mincho",
        "meiryo",
        "ms gothic",
        "ms mincho",
        "yu gothic",
        "yu mincho",
        // Generic CJK
        "noto sans cjk",
        "noto serif cjk",
        "source han",
        "han sans",
        "han serif",
        // Direct Chinese characters in name (some fonts have this)
        "宋体",
        "黑体",
        "楷体",
        "仿宋",
    ];

    chinese_indicators
        .iter()
        .any(|indicator| name_lower.contains(indicator))
}

/// Apply a specific font to the application
///
/// This function loads the specified font and configures it as the primary font
/// for the entire application. It follows the same loading pattern as `setup_fonts()`.
///
/// # Arguments
/// * `font_name` - The family name of the font to apply
///
/// # Returns
/// A configured `FontDefinitions` instance with the specified font, or defaults if loading fails
pub fn apply_font(font_name: &str) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let source = font_kit::source::SystemSource::new();

    // Try to load the requested font
    if try_load_font(&mut fonts, &source, font_name) {
        tracing::info!("Applied font: {}", font_name);
    } else {
        // If the specific font fails, try fallback
        tracing::warn!("Failed to load font '{}', using fallback", font_name);
        load_fallback_font(&mut fonts, &source);
    }

    fonts
}
