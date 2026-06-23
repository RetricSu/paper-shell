//! Built-in plugins compiled into Paper Shell.
//!
//! Built-in plugins are registered here and handed any configuration they need
//! at construction time. To add a new built-in plugin, implement
//! [`crate::plugin::Plugin`] in a submodule and push an instance in
//! [`builtin_plugins`].

pub mod github_publish;

use super::Plugin;
use std::sync::Arc;

/// Constructs the list of built-in plugins in the order they should appear in
/// the menu.
pub fn builtin_plugins(
    github_publish: github_publish::GithubPublishConfig,
) -> Vec<Arc<dyn Plugin>> {
    vec![Arc::new(github_publish::GithubPublishPlugin::new(
        github_publish,
    ))]
}
