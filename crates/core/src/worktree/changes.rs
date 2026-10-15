use super::session::{command, git};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn git_raw(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = command(cwd, args)?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Clone, Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    pub binary: bool,
    pub status: String,
}

#[derive(Serialize)]
pub struct RepositoryChanges {
    pub workspace_path: String,
    pub is_repository: bool,
    pub branch: Option<String>,
    pub github_url: Option<String>,
    pub compare_url: Option<String>,
    pub added: usize,
    pub removed: usize,
    pub files: Vec<ChangedFile>,
}

fn base(cwd: &Path) -> Result<String> {
    match git(cwd, &["rev-parse", "--verify", "HEAD"]) {
        Ok(head) => Ok(head),
        Err(_) => git(cwd, &["hash-object", "-t", "tree", "--stdin"]),
    }
}

fn root(cwd: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(git(cwd, &["rev-parse", "--show-toplevel"])?))
}

fn untracked_stats(path: &Path) -> Result<(usize, bool)> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.is_dir() {
        return Ok((0, true));
    }
    let mut file = std::fs::File::open(path)?;
    let mut buffer = [0u8; 65536];
    let mut lines = 0;
    let mut last = None;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if buffer[..count].contains(&0) {
            return Ok((0, true));
        }
        lines += buffer[..count]
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count();
        last = Some(buffer[count - 1]);
    }
    Ok((
        lines + usize::from(last.is_some() && last != Some(b'\n')),
        false,
    ))
}

fn github_url(remote: &str) -> Option<String> {
    let path = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))?;
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || !p
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
    {
        return None;
    }
    Some(format!("https://github.com/{path}"))
}

pub fn summary(cwd: &Path) -> Result<RepositoryChanges> {
    let state = super::session::branches(cwd)?;
    let mut result = RepositoryChanges {
        workspace_path: cwd.to_string_lossy().into_owned(),
        is_repository: state.is_repository,
        branch: state.current_branch,
        github_url: None,
        compare_url: None,
        added: 0,
        removed: 0,
        files: vec![],
    };
    if !result.is_repository {
        return Ok(result);
    }
    let repo = root(cwd)?;
    let base = base(&repo)?;
    let mut files = BTreeMap::new();
    let stats = git_raw(
        &repo,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--numstat",
            "-z",
            &base,
            "--",
        ],
    )?;
    for row in stats.split('\0').filter(|s| !s.is_empty()) {
        let mut parts = row.splitn(3, '\t');
        let added = parts.next().context("Invalid Git diff")?;
        let removed = parts.next().context("Invalid Git diff")?;
        let path = parts.next().context("Invalid Git filename")?;
        files.insert(
            path.to_owned(),
            ChangedFile {
                path: path.into(),
                added: added.parse().unwrap_or(0),
                removed: removed.parse().unwrap_or(0),
                binary: added == "-",
                status: "modified".into(),
            },
        );
    }
    // Name/status includes mode-only changes, empty files, deletions and conflicts.
    let names = git_raw(
        &repo,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--name-status",
            "-z",
            &base,
            "--",
        ],
    )?;
    let mut parts = names.split('\0').filter(|s| !s.is_empty());
    while let (Some(status), Some(path)) = (parts.next(), parts.next()) {
        let file = files.entry(path.into()).or_insert(ChangedFile {
            path: path.into(),
            added: 0,
            removed: 0,
            binary: false,
            status: String::new(),
        });
        file.status = match status {
            "A" => "added",
            "D" => "deleted",
            "U" => "conflict",
            "T" => "type",
            _ => "modified",
        }
        .into();
    }
    let status = git_raw(
        &repo,
        &["status", "--porcelain=v1", "--untracked-files=no", "-z"],
    )?;
    let mut entries = status.split('\0').filter(|s| !s.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let code = &entry[..2];
        let path = &entry[3..];
        if code.contains('R') || code.contains('C') {
            entries.next();
        }
        let file = files.entry(path.into()).or_insert(ChangedFile {
            path: path.into(),
            added: 0,
            removed: 0,
            binary: false,
            status: "modified".into(),
        });
        if code.contains('U') || code == "AA" || code == "DD" {
            file.status = "conflict".into();
        }
    }
    let untracked = git_raw(&repo, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for path in untracked.split('\0').filter(|s| !s.is_empty()) {
        let full = repo.join(path);
        // Never follow untracked symlinks out of the working tree.
        let (added, binary) = untracked_stats(&full)?;
        files.insert(
            path.into(),
            ChangedFile {
                path: path.into(),
                added,
                removed: 0,
                binary,
                status: "untracked".into(),
            },
        );
    }
    result.files = files.into_values().collect();
    result.added = result.files.iter().map(|f| f.added).sum();
    result.removed = result.files.iter().map(|f| f.removed).sum();
    result.github_url = git(&repo, &["remote", "get-url", "origin"])
        .ok()
        .and_then(|url| github_url(&url));
    if let (Some(url), Some(branch)) = (&result.github_url, &result.branch) {
        result.compare_url = Some(format!(
            "{url}/compare/{}?expand=1",
            urlencoding::encode(branch)
        ));
    }
    Ok(result)
}

#[derive(Serialize)]
pub struct FileDiff {
    pub patch: String,
    pub notice: Option<String>,
}

pub fn diff(cwd: &Path, path: &str) -> Result<FileDiff> {
    let repo = root(cwd)?;
    ensure!(
        !path.is_empty()
            && Path::new(path)
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
        "Invalid repository file path"
    );
    let status = git_raw(
        &repo,
        &[
            "--literal-pathspecs",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "-z",
            "--",
            path,
        ],
    )?;
    let entry = status
        .split('\0')
        .find(|entry| entry.get(3..) == Some(path))
        .context("File is no longer changed; refresh the list")?;
    if entry.starts_with("?? ") {
        let full = repo.join(path);
        let metadata = std::fs::symlink_metadata(&full)?;
        if metadata.file_type().is_symlink() || metadata.is_dir() || metadata.len() > 2_000_000 {
            return Ok(FileDiff {
                patch: String::new(),
                notice: Some("Symbolic link or large file; inline preview unavailable".into()),
            });
        }
        ensure!(
            !std::fs::symlink_metadata(&full)?.file_type().is_symlink(),
            "Cannot preview a symbolic link"
        );
        ensure!(
            full.canonicalize()?.starts_with(repo.canonicalize()?),
            "File is outside the repository"
        );
        let bytes = std::fs::read(full)?;
        if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
            return Ok(FileDiff {
                patch: String::new(),
                notice: Some("Binary file; inline preview unavailable".into()),
            });
        }
        let content = String::from_utf8_lossy(&bytes);
        let mut patch = format!("@@ -0,0 +1,{} @@\n", content.lines().count());
        for line in content.lines() {
            patch.push('+');
            patch.push_str(line);
            patch.push('\n');
        }
        return Ok(FileDiff {
            patch,
            notice: None,
        });
    }
    let base = base(&repo)?;
    let output = command(
        &repo,
        &[
            "--literal-pathspecs",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--unified=3",
            &base,
            "--",
            path,
        ],
    )?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let patch = String::from_utf8_lossy(&output.stdout);
    if patch.contains("Binary files ") {
        return Ok(FileDiff {
            patch: String::new(),
            notice: Some("Binary file; inline preview unavailable".into()),
        });
    }
    if patch.len() > 2_000_000 {
        return Ok(FileDiff {
            patch: String::new(),
            notice: Some("Diff is too large for inline preview".into()),
        });
    }
    Ok(FileDiff {
        patch: patch.into_owned(),
        notice: None,
    })
}

pub fn open_workspace(cwd: &Path, action: &str) -> Result<()> {
    match action {
        "explorer" => {
            open::that(cwd)?;
        }
        "github" | "compare" => {
            let info = summary(cwd)?;
            let url = if action == "github" {
                info.github_url
            } else {
                info.compare_url
            };
            open::that(url.context("A GitHub origin and a named branch are required")?)?;
        }
        "terminal" => {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                std::process::Command::new("powershell.exe")
                    .args(["-NoLogo", "-NoProfile", "-NoExit"])
                    .current_dir(cwd)
                    .creation_flags(0x00000010)
                    .spawn()?;
            }
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("open")
                    .args(["-a", "Terminal"])
                    .arg(cwd)
                    .spawn()?;
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                std::process::Command::new("x-terminal-emulator")
                    .current_dir(cwd)
                    .spawn()?;
            }
        }
        _ => anyhow::bail!("Unknown workspace action"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]).unwrap();
        git(dir.path(), &["config", "user.name", "Test"]).unwrap();
        git(dir.path(), &["config", "user.email", "test@example.com"]).unwrap();
        for file in ["modified.txt", "deleted.txt", "staged.txt", "canceled.txt"] {
            std::fs::write(dir.path().join(file), "old\n").unwrap();
        }
        git(dir.path(), &["add", "."]).unwrap();
        git(dir.path(), &["commit", "-m", "initial"]).unwrap();
        dir
    }
    #[test]
    fn lists_staged_unstaged_untracked_deleted_binary_and_canceled_changes() {
        let dir = fixture();
        let root = dir.path();
        std::fs::write(root.join("modified.txt"), "new\nmore\n").unwrap();
        std::fs::remove_file(root.join("deleted.txt")).unwrap();
        std::fs::write(root.join("staged.txt"), "staged\n").unwrap();
        std::fs::write(root.join("canceled.txt"), "staged\n").unwrap();
        git(root, &["add", "staged.txt", "canceled.txt"]).unwrap();
        std::fs::write(root.join("canceled.txt"), "old\n").unwrap();
        std::fs::write(root.join("new file 中文.txt"), "new\n").unwrap();
        std::fs::write(root.join("binary.png"), [0, 1, 2]).unwrap();
        std::fs::write(root.join("empty.txt"), "").unwrap();
        let state = summary(root).unwrap();
        assert_eq!(state.files.len(), 7);
        assert_eq!(state.added, 4);
        assert_eq!(state.removed, 3);
        assert_eq!(
            state
                .files
                .iter()
                .find(|f| f.path == "deleted.txt")
                .unwrap()
                .status,
            "deleted"
        );
        assert!(
            state
                .files
                .iter()
                .find(|f| f.path == "binary.png")
                .unwrap()
                .binary
        );
        assert!(diff(root, "binary.png").unwrap().notice.is_some());
        assert!(diff(root, "new file 中文.txt")
            .unwrap()
            .patch
            .contains("+new"));
        assert!(diff(root, "staged.txt").unwrap().patch.contains("+staged"));
        assert!(diff(root, "modified.txt").unwrap().patch.contains("+more"));
        assert!(diff(root, "../secret").is_err());
        assert!(diff(root, "--output=evil").is_err());
    }
    #[test]
    fn unborn_repo_and_worktrees_use_their_own_changes() {
        let empty = tempfile::tempdir().unwrap();
        git(empty.path(), &["init", "-b", "main"]).unwrap();
        std::fs::write(empty.path().join("new.txt"), "first\n").unwrap();
        git(empty.path(), &["add", "."]).unwrap();
        assert_eq!(summary(empty.path()).unwrap().added, 1);
        let source = fixture();
        let worktree = super::super::session::prepare(source.path(), Some("main"), true).unwrap();
        std::fs::write(worktree.path.join("modified.txt"), "only in worktree\n").unwrap();
        assert!(summary(source.path()).unwrap().files.is_empty());
        assert_eq!(summary(&worktree.path).unwrap().files.len(), 1);
        std::fs::write(worktree.path.join("modified.txt"), "old\n").unwrap();
        worktree.cleanup();
    }
    #[test]
    fn github_links_are_normalized_and_do_not_accept_other_hosts_or_credentials() {
        assert_eq!(
            github_url("git@github.com:org/repo.git").as_deref(),
            Some("https://github.com/org/repo")
        );
        assert_eq!(
            github_url("https://github.com/org/repo.git").as_deref(),
            Some("https://github.com/org/repo")
        );
        assert!(github_url("https://token@github.com/org/repo").is_none());
        assert!(github_url("https://github.com.evil/org/repo").is_none());
        assert!(github_url("https://github.com/../repo").is_none());
    }
}
