//! Paper Shell library
//!
//! This library exports the configuration module for use in examples and tests.

pub mod app;
pub mod backend;
mod backup_credentials;
pub mod comments;
pub mod config;
pub mod constant;
pub mod escape_monitor;
pub mod export;
pub mod file;
pub mod messages;
#[cfg(target_os = "macos")]
pub mod open_with;
pub mod plugin;
pub mod process_env;
pub mod skin;
pub mod statistics;
pub mod style;
pub mod ui;
pub mod updater;
