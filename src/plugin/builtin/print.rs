//! Print plugin: send the current editor content to a system printer.

use crate::plugin::{Plugin, PluginContext, PluginError, PluginMetadata};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DEFAULT_MARGIN_POINTS: u16 = 72;

pub struct PrintPlugin;

impl PrintPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for PrintPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "print".to_string(),
            name: "打印当前文档".to_string(),
            description: "调用系统默认打印机打印当前编辑器内容".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            author: "Paper Shell".to_string(),
        }
    }

    fn run(&self, ctx: &PluginContext) -> Result<String, PluginError> {
        if ctx.content.trim().is_empty() {
            return Err(PluginError::Config("当前文档为空，无法打印".to_string()));
        }

        let print_file = write_print_snapshot(ctx)?;
        let document_name = document_name(ctx);
        let margin = ctx.print_margin_points.unwrap_or(DEFAULT_MARGIN_POINTS);

        print_with_system_command(&print_file, &document_name, ctx.printer.as_deref(), margin)?;

        let printer = ctx.printer.as_deref().unwrap_or("默认打印机");
        Ok(format!("已发送到 {printer}：{document_name}"))
    }
}

pub fn available_printers() -> Vec<String> {
    available_printers_from_system()
}

fn write_print_snapshot(ctx: &PluginContext) -> Result<PathBuf, PluginError> {
    let extension = ctx
        .file_path
        .as_ref()
        .and_then(|path| path.extension())
        .and_then(|extension| extension.to_str())
        .unwrap_or("txt");
    let file_name = format!("paper-shell-print-{}.{}", std::process::id(), extension);
    let path = std::env::temp_dir().join(file_name);

    fs::write(&path, &ctx.content)?;
    Ok(path)
}

fn document_name(ctx: &PluginContext) -> String {
    ctx.file_path
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("未命名文档")
        .to_string()
}

#[cfg(target_os = "macos")]
fn print_with_system_command(
    path: &PathBuf,
    document_name: &str,
    printer: Option<&str>,
    margin_points: u16,
) -> Result<(), PluginError> {
    let mut command = Command::new("lp");
    command.arg("-t").arg(document_name);
    if let Some(printer) = printer.filter(|value| !value.trim().is_empty()) {
        command.arg("-d").arg(printer);
    }
    add_margin_options(&mut command, margin_points);
    let output = command
        .arg(path)
        .output()
        .map_err(|_| PluginError::Config("未找到 lp 打印命令".to_string()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(PluginError::Execution(if stderr.is_empty() {
        "打印命令执行失败".to_string()
    } else {
        stderr
    }))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn print_with_system_command(
    path: &PathBuf,
    document_name: &str,
    printer: Option<&str>,
    margin_points: u16,
) -> Result<(), PluginError> {
    let mut command = Command::new("lpr");
    command.arg("-T").arg(document_name);
    if let Some(printer) = printer.filter(|value| !value.trim().is_empty()) {
        command.arg("-P").arg(printer);
    }
    add_margin_options(&mut command, margin_points);
    let output = command
        .arg(path)
        .output()
        .map_err(|_| PluginError::Config("未找到 lpr 打印命令".to_string()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(PluginError::Execution(if stderr.is_empty() {
        "打印命令执行失败".to_string()
    } else {
        stderr
    }))
}

#[cfg(windows)]
fn print_with_system_command(
    path: &PathBuf,
    _document_name: &str,
    _printer: Option<&str>,
    _margin_points: u16,
) -> Result<(), PluginError> {
    let output = Command::new("notepad")
        .arg("/p")
        .arg(path)
        .output()
        .map_err(|_| PluginError::Config("未找到 notepad 打印命令".to_string()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(PluginError::Execution(if stderr.is_empty() {
        "打印命令执行失败".to_string()
    } else {
        stderr
    }))
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
fn add_margin_options(command: &mut Command, margin_points: u16) {
    let margin = margin_points.to_string();
    for option in ["page-left", "page-right", "page-top", "page-bottom"] {
        command.arg("-o").arg(format!("{option}={margin}"));
    }
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
fn available_printers_from_system() -> Vec<String> {
    let output = Command::new("lpstat").arg("-e").output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(windows)]
fn available_printers_from_system() -> Vec<String> {
    Vec::new()
}
