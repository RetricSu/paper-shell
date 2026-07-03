//! Process environment initialization helpers.

#[cfg(target_os = "macos")]
use std::collections::HashSet;
use std::env;
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Command;

/// Initializes process environment for GUI-launched app bundles.
///
/// On macOS, Finder-launched apps often miss user shell PATH entries
/// (for example Homebrew), which breaks CLI integrations such as `gh`.
#[cfg(target_os = "macos")]
pub fn initialize_process_path() {
    let mut merged = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    if let Some(login_shell_path) = read_login_shell_path() {
        merge_path_entries(&mut merged, &mut seen, &login_shell_path);
    }

    if let Ok(current_path) = env::var("PATH") {
        merge_path_entries(&mut merged, &mut seen, &current_path);
    }

    for dir in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
    ] {
        if Path::new(dir).exists() && seen.insert(dir.to_string()) {
            merged.push(dir.to_string());
        }
    }

    if !merged.is_empty() {
        // Safe here because this runs at process startup before worker threads.
        unsafe {
            env::set_var("PATH", merged.join(":"));
        }
        tracing::info!("Initialized process PATH with {} entries", merged.len());
    }
}

#[cfg(target_os = "macos")]
fn read_login_shell_path() -> Option<String> {
    let shell = env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string());

    let output = Command::new(&shell)
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .output()
        .ok()?;

    if !output.status.success() {
        tracing::warn!("Failed to read login shell PATH via {}", shell);
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

#[cfg(target_os = "macos")]
fn merge_path_entries(target: &mut Vec<String>, seen: &mut HashSet<String>, source: &str) {
    for entry in source.split(':') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            target.push(trimmed.to_string());
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn initialize_process_path() {}
