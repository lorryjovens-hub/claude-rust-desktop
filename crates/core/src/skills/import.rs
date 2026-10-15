//! Explicit, nonexecuting import of legacy SKILL.md directories.
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

const MARKER: &str = ".legacy-import-source.json";
#[derive(Debug, Serialize)]
pub struct Report {
    pub source: String,
    pub destination: String,
    pub preview: bool,
    pub imported: bool,
    pub skipped: bool,
    pub skills: Vec<String>,
    pub files: usize,
    pub backup: Option<String>,
}
fn resolve_future(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    ensure!(
        !absolute
            .components()
            .any(|c| matches!(c, Component::ParentDir)),
        "Target cannot contain parent traversal"
    );
    let mut ancestor = absolute.as_path();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        suffix.push(
            ancestor
                .file_name()
                .context("Invalid target component")?
                .to_os_string(),
        );
        ancestor = ancestor.parent().context("No existing target ancestor")?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for name in suffix.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}
fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(18)
    {
        let entry = entry?;
        let path = entry.path();
        ensure!(
            !entry.file_type().is_symlink(),
            "Skill directory contains a symbolic link"
        );
        ensure!(
            path.canonicalize()?.starts_with(root),
            "Skill path escapes directory"
        );
        let relative = path.strip_prefix(root)?;
        ensure!(
            relative.components().count() <= 16,
            "Skill directory nesting exceeds 16 levels"
        );
        if entry.file_type().is_dir() {
            continue;
        }
        ensure!(
            entry.file_type().is_file(),
            "Skill package contains a special file"
        );
        ensure!(
            relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
            "Invalid skill path"
        );
        let name = relative.to_str().context("Skill paths must be UTF-8")?;
        ensure!(
            !name.contains(':'),
            "Skill paths cannot contain alternate data streams"
        );
        total += entry.metadata()?.len();
        ensure!(
            total <= 32_000_000 && files.len() < 1000,
            "Skills import exceeds 32 MB or 1000 files"
        );
        files.insert(relative.to_path_buf(), std::fs::read(path)?);
    }
    Ok(files)
}
fn write_snapshot(root: &Path, files: &BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    for (path, bytes) in files {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().context("Missing skill parent")?)?;
        std::fs::write(target, bytes)?;
    }
    Ok(())
}
pub fn import_legacy(source: &Path, target: &Path, preview: bool) -> Result<Report> {
    ensure!(
        !std::fs::symlink_metadata(source)?.file_type().is_symlink(),
        "Source must not be a symbolic link"
    );
    let source = source.canonicalize()?;
    ensure!(source.is_dir(), "Expected a skill directory or collection");
    let parent = resolve_future(target.parent().context("Target parent missing")?)?;
    let target = parent.join(target.file_name().context("Target name missing")?);
    if let Ok(meta) = std::fs::symlink_metadata(&target) {
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "Target must be a regular directory"
        );
    }
    ensure!(
        !source.starts_with(&target) && !target.starts_with(&source),
        "Source and target skill directories must not overlap"
    );
    let mut files = snapshot(&source)?;
    ensure!(
        files.len() < 1000 && files.values().map(Vec::len).sum::<usize>() <= 31_000_000,
        "Source exceeds limits after reserving import metadata space"
    );
    ensure!(
        !files.contains_key(Path::new(MARKER)),
        "Source is already an imported namespace; select its original source"
    );
    let mut skills = Vec::new();
    for (path, bytes) in &files {
        if path.file_name().is_some_and(|n| n == "SKILL.md") {
            ensure!(
                path.components().count() <= 3,
                "SKILL.md is too deeply nested for the shared skill loader"
            );
            let text = std::str::from_utf8(bytes).context("SKILL.md must be UTF-8")?;
            ensure!(!text.trim().is_empty(), "SKILL.md is empty");
            skills.push(path.to_string_lossy().into_owned());
        }
    }
    ensure!(!skills.is_empty(), "No SKILL.md found in source directory");
    // FNV-1a is a stable namespace identifier, not an integrity hash. Exact file and source-marker
    // comparisons below detect content differences and namespace collisions before any mutation.
    let source_text = source.to_string_lossy().into_owned();
    let hash = source_text.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    let destination = target.join(format!("legacy-{hash:016x}"));
    files.insert(
        PathBuf::from(MARKER),
        serde_json::to_vec(&serde_json::json!({"source":source_text}))?,
    );
    let report = |skipped: bool, backup: Option<String>| Report {
        source: source_text.clone(),
        destination: destination.to_string_lossy().into_owned(),
        preview,
        imported: !preview && !skipped,
        skipped,
        skills: skills.clone(),
        files: files.len() - 1,
        backup,
    };
    let check = || -> Result<bool> {
        if !destination.exists() {
            return Ok(false);
        }
        ensure!(
            snapshot(&destination)? == files,
            "Skill namespace conflicts with existing content; source was not imported"
        );
        Ok(true)
    };
    if preview {
        return Ok(report(check()?, None));
    }
    std::fs::create_dir_all(&parent)?;
    let lock_path = parent.join(".skills-import.lock");
    if let Ok(meta) = std::fs::symlink_metadata(&lock_path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Invalid skills lock file"
        );
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    lock.try_lock()
        .context("Another skills import is running")?;
    if check()? {
        return Ok(report(true, None));
    }
    let backup_dir = parent
        .join("backups")
        .join(format!("skills-{}", uuid::Uuid::new_v4()));
    let old = if target.exists() {
        snapshot(&target)?
    } else {
        BTreeMap::new()
    };
    std::fs::create_dir_all(&backup_dir)?;
    write_snapshot(&backup_dir, &old)?;
    // Build a complete immutable namespace on the same volume, then publish with one rename.
    // No preexisting skill is overwritten or removed, so a failed copy needs no target rollback.
    let staging = tempfile::tempdir_in(&parent)?;
    write_snapshot(staging.path(), &files)?;
    std::fs::create_dir_all(&target)?;
    ensure!(
        !destination.exists(),
        "Skill namespace appeared during import"
    );
    std::fs::rename(staging.path(), &destination)?;
    Ok(report(
        false,
        Some(backup_dir.to_string_lossy().into_owned()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_does_not_create_missing_data_directory() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "# Skill").unwrap();
        let target = dir.path().join("missing/data/skills");
        assert!(import_legacy(&source, &target, true).unwrap().preview);
        assert!(!dir.path().join("missing").exists());
        assert!(import_legacy(&source, &target, false).unwrap().imported);
        assert!(target.is_dir());
    }
    #[tokio::test]
    async fn collection_import_is_previewable_idempotent_and_discoverable() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old");
        let target = dir.path().join("skills");
        std::fs::create_dir_all(source.join("group/alpha/scripts")).unwrap();
        let original = "---\nname: alpha\ndescription: Imported fixture\n---\n# Instructions\n";
        std::fs::write(source.join("group/alpha/SKILL.md"), original).unwrap();
        std::fs::write(
            source.join("group/alpha/scripts/run.py"),
            "raise Exception('must not execute')",
        )
        .unwrap();
        let preview = import_legacy(&source, &target, true).unwrap();
        assert!(preview.preview);
        assert!(!target.exists());
        assert!(!dir.path().join("backups").exists());
        let imported = import_legacy(&source, &target, false).unwrap();
        assert!(imported.imported);
        assert!(Path::new(&imported.backup.unwrap()).is_dir());
        assert!(import_legacy(&source, &target, false).unwrap().skipped);
        let manager = super::super::SkillsManager::with_path(target.clone());
        let loaded = manager.load_skills().await.unwrap();
        let skill = loaded.iter().find(|s| s.name == "alpha").unwrap();
        assert!(manager
            .find_skill_path(&skill.id)
            .unwrap()
            .join("scripts/run.py")
            .exists());
        std::fs::write(source.join("group/alpha/SKILL.md"), "changed").unwrap();
        assert!(import_legacy(&source, &target, false).is_err());
        assert_eq!(
            std::fs::read_to_string(Path::new(&imported.destination).join("group/alpha/SKILL.md"))
                .unwrap(),
            original
        );
    }
    #[test]
    fn root_skill_and_overlap_validation() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("single");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "# Single skill").unwrap();
        assert_eq!(
            import_legacy(&source, &dir.path().join("skills"), true)
                .unwrap()
                .skills,
            ["SKILL.md"]
        );
        assert!(import_legacy(&source, &source.join("nested"), true).is_err());
    }
}
