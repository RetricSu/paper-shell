//! Example demonstrating the configuration system
//!
//! Run with: cargo run --example config_demo

use paper_shell::config::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Paper Shell Configuration Demo ===\n");

    // Load or create default config
    let config = Config::load()?;

    println!("Current settings:");
    println!("  Theme: {}", config.settings.theme);
    println!(
        "  Auto-save interval: {} seconds",
        config.settings.autosave_interval
    );
    println!("  Font size: {}", config.settings.font_size);

    println!("\nData directory: {}", config.data_dir().display());
    println!("Config file: {}", Config::config_path()?.display());

    println!("\n✅ Configuration loaded successfully!");
    println!("You can modify the config file manually or use Config::save() to persist changes.");

    Ok(())
}
