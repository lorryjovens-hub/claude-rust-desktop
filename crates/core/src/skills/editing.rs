use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::Write,
    path::{Component, Path, PathBuf},
};

pub(super) fn safe_existing(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let full = path.canonicalize()?;
    ensure!(
        full.starts_with(&root),
        "Skill path escapes skills directory"
    );
    let mut current = path.to_path_buf();
    loop {
        ensure!(
            !std::fs::symlink_metadata(&current)?
                .file_type()
                .is_symlink(),
            "Symbolic links are not allowed in skills"
        );
        if current.canonicalize()? == root {
            break;
        }
        current = current
            .parent()
            .context("Skill path has no parent")?
            .to_path_buf();
    }
    Ok(full)
}
pub(super) fn valid_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.chars().count() <= 100
            && !name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
            && !name.ends_with(['.', ' '])
            && !name.chars().any(char::is_control),
        "Invalid skill directory name"
    );
    ensure!(
        Path::new(name).components().count() == 1
            && Path::new(name)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "Skill name must be one normal directory component"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::SkillsManager;
    use serde_json::json;

    #[test]
    fn creation_rejects_traversal_and_preserves_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let manager = SkillsManager::with_path(temp.path().join("skills"));
        for name in ["../escape", "..", "a/b", "a\\b", "a:stream", "bad."] {
            assert!(manager.create_skill(name, "description", "body").is_err());
        }
        let skill = manager
            .create_skill(
                "safe",
                "colon: value\nsecond line",
                "---\nenabled: false\n---\nOriginal",
            )
            .unwrap();
        assert!(manager.create_skill("safe", "changed", "changed").is_err());
        assert!(manager.get_skill_content(&skill.id, "../SKILL.md").is_err());
        assert!(manager.get_skill_content(&skill.id, "C:\\secret").is_err());
        let original = manager.get_skill_content(&skill.id, "SKILL.md").unwrap();
        assert_eq!(
            split(&original).unwrap().0["description"].as_str(),
            Some("colon: value\nsecond line")
        );
        manager
            .update_skill(
                &skill.id,
                HashMap::from([
                    ("content".into(), json!("New body")),
                    ("description".into(), json!("New: description")),
                ]),
            )
            .unwrap();
        let updated = manager.get_skill_content(&skill.id, "SKILL.md").unwrap();
        let (header, body) = split(&updated).unwrap();
        assert_eq!(header["enabled"].as_bool(), Some(false));
        assert_eq!(header["name"].as_str(), Some("safe"));
        assert_eq!(header["description"].as_str(), Some("New: description"));
        assert_eq!(body, "New body");
        assert!(manager
            .update_skill(
                &skill.id,
                HashMap::from([("enabled".into(), json!("false"))])
            )
            .is_err());
        assert_eq!(
            manager.get_skill_content(&skill.id, "SKILL.md").unwrap(),
            updated
        );
    }

    #[test]
    fn empty_frontmatter_and_complete_document_updates_are_supported() {
        assert_eq!(split("---\n---\nbody").unwrap().1, "body");
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("SKILL.md");
        std::fs::write(&path, document("first", "desc", "old").unwrap()).unwrap();
        edit(
            &path,
            &HashMap::from([(
                "content".into(),
                json!("---\nname: second\nenabled: false\n---\nnew"),
            )]),
        )
        .unwrap();
        let (header, body) = split(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(header["name"].as_str(), Some("second"));
        assert_eq!(body, "new");
    }
}
pub(super) fn relative_file(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty()
            && !name.contains(':')
            && Path::new(name)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "Skill file path must stay within its directory"
    );
    safe_existing(root, &root.join(name))
}
fn split(content: &str) -> Result<(serde_yaml::Mapping, String)> {
    let normalized = content.replace("\r\n", "\n");
    if let Some(rest) = normalized.strip_prefix("---\n") {
        if rest == "---" || rest.starts_with("---\n") {
            return Ok((
                serde_yaml::Mapping::new(),
                rest.strip_prefix("---\n").unwrap_or("").to_string(),
            ));
        }
        let (header, body) = rest
            .split_once("\n---\n")
            .or_else(|| rest.strip_suffix("\n---").map(|h| (h, "")))
            .context("Unterminated skill frontmatter")?;
        let value: serde_yaml::Value = serde_yaml::from_str(header)?;
        Ok((
            value
                .as_mapping()
                .context("Skill frontmatter must be a mapping")?
                .clone(),
            body.to_string(),
        ))
    } else {
        Ok((serde_yaml::Mapping::new(), content.to_string()))
    }
}
pub(super) fn document(name: &str, description: &str, content: &str) -> Result<String> {
    let (mut header, body) = split(content)?;
    header.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(name.into()),
    );
    header.insert(
        serde_yaml::Value::String("description".into()),
        serde_yaml::Value::String(description.into()),
    );
    Ok(format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&header)?,
        body
    ))
}
pub(super) fn edit(path: &Path, updates: &HashMap<String, Value>) -> Result<()> {
    ensure!(
        updates
            .keys()
            .all(|k| matches!(k.as_str(), "name" | "description" | "content" | "enabled")),
        "Unsupported skill update field"
    );
    let parent = path.parent().context("Skill parent missing")?;
    let lock_path = parent.join(".skill-edit.lock");
    if let Ok(m) = std::fs::symlink_metadata(&lock_path) {
        ensure!(
            m.is_file() && !m.file_type().is_symlink(),
            "Invalid skill edit lock"
        );
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    lock.try_lock()
        .context("Skill is being edited by another process")?;
    let old = std::fs::read_to_string(path)?;
    let base = updates
        .get("content")
        .map(|v| v.as_str().context("content must be a string"))
        .transpose()?
        .unwrap_or(&old);
    ensure!(base.len() <= 2_000_000, "Skill content exceeds 2 MB");
    let (mut header, body) = split(base)?;
    if updates.contains_key("content") && !base.starts_with("---\n") && !base.starts_with("---\r\n")
    {
        header = split(&old)?.0;
    }
    for field in ["name", "description"] {
        if let Some(value) = updates.get(field) {
            let value = value
                .as_str()
                .context("Skill text field must be a string")?;
            header.insert(
                serde_yaml::Value::String(field.into()),
                serde_yaml::Value::String(value.into()),
            );
        }
    }
    if let Some(value) = updates.get("enabled") {
        header.insert(
            serde_yaml::Value::String("enabled".into()),
            serde_yaml::Value::Bool(value.as_bool().context("enabled must be boolean")?),
        );
    }
    let output = format!("---\n{}---\n{}", serde_yaml::to_string(&header)?, body);
    ensure!(output.len() <= 2_000_000, "Skill document exceeds 2 MB");
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(output.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
