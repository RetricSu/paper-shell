//! Built-in plugins compiled into Paper Shell.
//!
//! Built-in plugins are registered here and handed any configuration they need
//! at construction time. To add a new built-in plugin, implement
//! [`crate::plugin::Plugin`] in a submodule and push an instance in
//! [`builtin_plugins`].

pub mod cloud_backup;
pub mod github_publish;
pub mod print;

use super::Plugin;
use std::sync::Arc;

/// IDs reserved by built-in plugins. External plugins must not use these IDs.
pub const RESERVED_PLUGIN_IDS: &[&str] = &["print", "cloud_backup", "github_publish"];

/// Constructs the list of built-in plugins in the order they should appear in
/// the menu.
pub fn builtin_plugins(
    github_publish: github_publish::GithubPublishConfig,
    cloud_backup: cloud_backup::CloudBackupConfig,
) -> Vec<Arc<dyn Plugin>> {
    vec![
        Arc::new(print::PrintPlugin::new()),
        Arc::new(cloud_backup::CloudBackupPlugin::new(cloud_backup)),
        Arc::new(github_publish::GithubPublishPlugin::new(github_publish)),
    ]
}
