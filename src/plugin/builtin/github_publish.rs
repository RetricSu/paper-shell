//! GitHub publish plugin: publish the current post through a PR workflow.
//!
//! This built-in plugin uses the `gh` and `git` CLIs to clone a target blog
//! repository into the app data directory, refresh or create a local checkout,
//! create a branch, write the current post into the configured folder, commit
//! the change, push it, and open a pull request. Authentication and GitHub API
//! details stay delegated to `gh`, keeping the plugin dependency-light.
//!
//! [`gh`]: https://cli.github.com

use crate::plugin::{Plugin, PluginContext, PluginError, PluginMetadata};
use chrono::{Datelike, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
    pub label: String,
    pub dir: String,
}

/// Persistent configuration for the GitHub publish plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GithubPublishConfig {
    /// Target repository in `owner/repo` form, e.g. `RetricSu/blog`.
    pub repo: String,
    /// Base branch to open the PR against.
    pub base_branch: String,
    /// Commit message template. `{filename}` is replaced with the file name.
    pub commit_message: String,
    /// PR title template. `{filename}` is replaced with the file name.
    pub pr_title: String,
    /// Base content directory when no collections are configured.
    pub base_dir: String,
    /// Optional collection directory mappings shown in the publish dialog.
    pub collections: Vec<CollectionConfig>,
}

impl Default for GithubPublishConfig {
    fn default() -> Self {
        Self {
            repo: String::new(),
            base_branch: "main".to_string(),
            commit_message: "docs: publish {filename}".to_string(),
            pr_title: "Publish {filename}".to_string(),
            base_dir: String::new(),
            collections: Vec::new(),
        }
    }
}

/// The GitHub publish plugin.
pub struct GithubPublishPlugin {
    config: GithubPublishConfig,
}

impl GithubPublishPlugin {
    pub fn new(config: GithubPublishConfig) -> Self {
        Self { config }
    }
}

impl Plugin for GithubPublishPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "github_publish".to_string(),
            name: "GitHub 博客发布".to_string(),
            description:
                "通过 gh CLI 克隆博客仓库、创建分支并提交 PR，将当前文章发布到你的 GitHub 博客"
                    .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            author: "Paper Shell".to_string(),
        }
    }

    fn run(&self, ctx: &PluginContext) -> Result<String, PluginError> {
        let repo = self.config.repo.trim();
        if repo.is_empty() {
            return Err(PluginError::Config(
                "未设置博客仓库。请在配置文件中设置 github_publish.repo（格式 owner/repo），然后重新打开编辑器。"
                    .to_string(),
            ));
        }

        let path = ctx.file_path.as_ref().ok_or(PluginError::NoActiveFile)?;
        let title = ctx
            .title
            .as_ref()
            .ok_or(PluginError::Config("缺少标题".to_string()))?;
        let collection_dir = ctx
            .collection
            .as_ref()
            .ok_or(PluginError::Config("缺少分类目录".to_string()))?;
        let md_path = path.with_extension("md");
        let filename = md_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| PluginError::Execution("无法解析文件名".to_string()))?
            .to_string();

        ensure_gh_available()?;
        ensure_git_available()?;

        let base_branch = if self.config.base_branch.trim().is_empty() {
            "main"
        } else {
            self.config.base_branch.trim()
        };
        let clone_root = ctx.data_dir.join("github_publish");
        let clone_dir = clone_root.join(repo_safe_name(repo));
        fs::create_dir_all(&clone_root)?;

        prepare_clone(repo, base_branch, &clone_root, &clone_dir)?;

        let now = Local::now().naive_local();
        let branch_name = build_branch_name(&filename, now);
        run_command(
            "git",
            &[
                "checkout".to_string(),
                "-b".to_string(),
                branch_name.clone(),
            ],
            Some(&clone_dir),
        )?;

        let frontmatter = build_frontmatter(title, ctx.description.as_deref(), now);
        let target_path = build_target_path(collection_dir, &filename);
        let target_file = clone_dir.join(&target_path);
        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target_file, format!("{}{}", frontmatter, ctx.content))?;

        let commit_message = self.config.commit_message.replace("{filename}", &filename);
        let pr_title = self.config.pr_title.replace("{filename}", &filename);
        let pr_body = format!(
            "通过 Paper Shell 发布文章 `{filename}`。\n\n源文件：{}",
            path.display()
        );

        run_command(
            "git",
            &["add".to_string(), target_path.clone()],
            Some(&clone_dir),
        )?;
        run_command(
            "git",
            &[
                "commit".to_string(),
                "-m".to_string(),
                commit_message.clone(),
            ],
            Some(&clone_dir),
        )?;
        run_command(
            "git",
            &[
                "push".to_string(),
                "-u".to_string(),
                "origin".to_string(),
                branch_name.clone(),
            ],
            Some(&clone_dir),
        )?;
        let pr_url = run_command(
            "gh",
            &[
                "pr".to_string(),
                "create".to_string(),
                "--base".to_string(),
                base_branch.to_string(),
                "--head".to_string(),
                branch_name.clone(),
                "--title".to_string(),
                pr_title,
                "--body".to_string(),
                pr_body,
            ],
            Some(&clone_dir),
        )?;

        Ok(format!(
            "✅ 已创建 PR\n\n仓库：{repo}\n分支：{branch_name}\nPR：{}",
            pr_url.trim()
        ))
    }
}

/// Joins the configured target directory and the file name into a repo path.
fn build_target_path(target_dir: &str, filename: &str) -> String {
    let dir = target_dir.trim().trim_matches('/');
    if dir.is_empty() {
        filename.to_string()
    } else {
        format!("{dir}/{filename}")
    }
}

fn build_frontmatter(title: &str, description: Option<&str>, now: NaiveDateTime) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    let pub_date = format!(
        "{} {} {}",
        MONTHS[now.month0() as usize],
        now.day(),
        now.year()
    );
    let mut frontmatter = format!("---\ntitle: '{}'\npubDate: '{}'\n", title, pub_date);
    if let Some(description) = description {
        if !description.trim().is_empty() {
            frontmatter.push_str(&format!("description: '{}'\n", description));
        }
    }
    frontmatter.push_str("---\n\n");
    frontmatter
}

/// Builds a branch name from the file stem and current timestamp.
fn build_branch_name(filename: &str, now: NaiveDateTime) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("post")
        .trim();
    let sanitized = stem
        .chars()
        .map(|ch| {
            if ch.is_whitespace() {
                '-'
            } else if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = sanitized.trim_matches('-');
    let slug = if slug.is_empty() { "post" } else { slug };
    format!("post/{slug}-{}", now.format("%Y%m%d-%H%M%S"))
}

/// Converts `owner/repo` to a filesystem-safe directory name.
fn repo_safe_name(repo: &str) -> String {
    repo.replace('/', "-")
}

/// Verifies the `gh` CLI is installed and on PATH.
fn ensure_gh_available() -> Result<(), PluginError> {
    Command::new("gh")
        .arg("--version")
        .output()
        .map_err(|_| {
            PluginError::Config(
                "未找到 gh CLI。请先安装并登录 GitHub CLI：https://cli.github.com".to_string(),
            )
        })
        .map(|_| ())
}

/// Verifies the `git` CLI is installed and on PATH.
fn ensure_git_available() -> Result<(), PluginError> {
    Command::new("git")
        .arg("--version")
        .output()
        .map_err(|_| PluginError::Config("未找到 git CLI。请先安装 Git。".to_string()))
        .map(|_| ())
}

/// Prepares a clean local checkout for publishing.
fn prepare_clone(
    repo: &str,
    base_branch: &str,
    clone_root: &Path,
    clone_dir: &Path,
) -> Result<(), PluginError> {
    if is_git_repo(clone_dir) {
        run_command(
            "git",
            &["fetch".to_string(), "origin".to_string()],
            Some(clone_dir),
        )?;
        run_command(
            "git",
            &["checkout".to_string(), base_branch.to_string()],
            Some(clone_dir),
        )?;
        run_command(
            "git",
            &[
                "reset".to_string(),
                "--hard".to_string(),
                format!("origin/{base_branch}"),
            ],
            Some(clone_dir),
        )?;
        cleanup_old_post_branches(clone_dir);
        return Ok(());
    }

    if clone_dir.exists() {
        fs::remove_dir_all(clone_dir)?;
    }

    let mut args = vec![
        "repo".to_string(),
        "clone".to_string(),
        repo.to_string(),
        path_to_string(clone_dir)?,
        "--".to_string(),
        "--depth".to_string(),
        "1".to_string(),
    ];
    if base_branch != "main" {
        args.push("-b".to_string());
        args.push(base_branch.to_string());
    }

    run_command("gh", &args, Some(clone_root))?;
    Ok(())
}

/// Returns whether the directory already looks like a git checkout.
fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").is_dir()
}

/// Deletes stale local `post/*` branches. Safe because callers are on
/// `base_branch` at this point, so no `post/*` branch is checked out.
fn cleanup_old_post_branches(clone_dir: &Path) {
    let list = match Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/post/",
        ])
        .current_dir(clone_dir)
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => {
            tracing::warn!(
                "git for-each-ref failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            return;
        }
        Err(e) => {
            tracing::warn!("git for-each-ref spawn failed: {}", e);
            return;
        }
    };

    for branch in list.lines().map(str::trim).filter(|s| !s.is_empty()) {
        match Command::new("git")
            .args(["branch", "-D", branch])
            .current_dir(clone_dir)
            .output()
        {
            Ok(o) if !o.status.success() => tracing::warn!(
                "git branch -D {} failed: {}",
                branch,
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => tracing::warn!("git branch -D {} spawn failed: {}", branch, e),
            _ => {}
        }
    }
}

/// Runs a CLI command and returns trimmed stdout on success.
fn run_command(
    program: &str,
    args: &[String],
    current_dir: Option<&Path>,
) -> Result<String, PluginError> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }

    let output = command.output().map_err(|e| {
        tracing::error!("Failed to spawn {}: {}", program, e);
        PluginError::Execution(format!("{} 命令执行失败：{}", program, e))
    })?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr
    };
    tracing::error!("{} failed: {}", program, detail);
    Err(PluginError::Execution(format!(
        "{} 命令执行失败：{}",
        program, detail
    )))
}

fn path_to_string(path: &Path) -> Result<String, PluginError> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| PluginError::Execution("无法解析仓库路径".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn build_branch_name_uses_stem_spaces_and_timestamp() {
        let now = NaiveDate::from_ymd_opt(2026, 6, 23)
            .unwrap()
            .and_hms_opt(14, 5, 6)
            .unwrap();

        assert_eq!(
            build_branch_name("hello world.md", now),
            "post/hello-world-20260623-140506"
        );
    }

    #[test]
    fn build_branch_name_strips_non_ascii() {
        let now = NaiveDate::from_ymd_opt(2026, 6, 23)
            .unwrap()
            .and_hms_opt(14, 5, 6)
            .unwrap();

        assert_eq!(
            build_branch_name("博客流程.md", now),
            "post/post-20260623-140506"
        );
    }

    #[test]
    fn repo_safe_name_replaces_slash() {
        assert_eq!(repo_safe_name("RetricSu/blog"), "RetricSu-blog");
    }

    #[test]
    fn target_path_handles_empty_and_slashed_dirs() {
        assert_eq!(build_target_path("posts", "a.md"), "posts/a.md");
        assert_eq!(build_target_path("/posts/", "a.md"), "posts/a.md");
        assert_eq!(build_target_path("", "a.md"), "a.md");
        assert_eq!(build_target_path("  ", "a.md"), "a.md");
    }

    #[test]
    fn frontmatter_with_and_without_description() {
        let now = NaiveDate::from_ymd_opt(2026, 6, 23)
            .unwrap()
            .and_hms_opt(14, 5, 6)
            .unwrap();
        let with_desc = build_frontmatter("测试标题", Some("测试描述"), now);
        assert!(with_desc.contains("title: '测试标题'"));
        assert!(with_desc.contains("pubDate: 'Jun 23 2026'"));
        assert!(with_desc.contains("description: '测试描述'"));
        assert!(with_desc.starts_with("---\n"));
        assert!(with_desc.contains("---\n\n"));

        let no_desc = build_frontmatter("测试标题", None, now);
        assert!(!no_desc.contains("description"));
    }
}
