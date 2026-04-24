use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: Option<String>,
    #[serde(rename = "whenToUse")]
    pub when_to_use: Option<String>,
    pub enabled: bool,
    pub source: SkillSource,
    #[serde(rename = "loadedFrom")]
    pub loaded_from: String,
    #[serde(rename = "sourceDir")]
    pub source_dir: Option<String>,
    #[serde(rename = "isExample")]
    pub is_example: bool,
    pub files: Vec<SkillFile>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SkillSource {
    User,
    Project,
    Plugin,
    Bundled,
    Mcp,
}

impl SkillSource {
    pub fn as_str(&self) -> &str {
        match self {
            SkillSource::User => "user",
            SkillSource::Project => "project",
            SkillSource::Plugin => "plugin",
            SkillSource::Bundled => "bundled",
            SkillSource::Mcp => "mcp",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub children: Option<Vec<SkillFile>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmatterData {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "whenToUse")]
    pub when_to_use: Option<String>,
    #[serde(rename = "allowedTools")]
    pub allowed_tools: Option<Vec<String>>,
    pub model: Option<String>,
    #[serde(rename = "userInvocable")]
    pub user_invokable: Option<bool>,
}

pub struct SkillsManager {
    skills_dir: PathBuf,
}

impl SkillsManager {
    pub fn new() -> Self {
        let skills_dir = Self::get_skills_base_path();
        Self { skills_dir }
    }

    fn get_skills_base_path() -> PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".claude").join("skills")
    }

    pub fn get_skills_path(&self, source: &SkillSource) -> PathBuf {
        match source {
            SkillSource::User => self.skills_dir.clone(),
            SkillSource::Project => PathBuf::from(".claude/skills"),
            SkillSource::Plugin => PathBuf::from("plugin"),
            SkillSource::Bundled => self.skills_dir.join("bundled"),
            SkillSource::Mcp => self.skills_dir.join("mcp"),
        }
    }

    pub async fn load_skills(&self) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        let user_skills_path = self.get_skills_path(&SkillSource::User);
        if user_skills_path.exists() {
            skills.extend(self.scan_skills_dir(&user_skills_path, SkillSource::User)?);
        }

        let bundled_skills_path = self.get_skills_path(&SkillSource::Bundled);
        if bundled_skills_path.exists() {
            skills.extend(self.scan_skills_dir(&bundled_skills_path, SkillSource::Bundled)?);
        } else {
            self.install_bundled_skills()?;
            if bundled_skills_path.exists() {
                skills.extend(self.scan_skills_dir(&bundled_skills_path, SkillSource::Bundled)?);
            }
        }

        Ok(skills)
    }

    pub fn install_bundled_skills(&self) -> Result<()> {
        let bundled_path = self.get_skills_path(&SkillSource::Bundled);
        std::fs::create_dir_all(&bundled_path)?;

        let skill_creator_md = r#"---
name: skill-creator
description: A toolkit for creating, evaluating, and improving Claude skills
whenToUse: When you want to build a new skill, evaluate an existing one, or generate synthetic training data for skills
---

# Skill Creator

A comprehensive toolkit for creating, evaluating, and improving Claude skills.

## Usage

This skill provides utilities for:
- Creating new skills from templates
- Evaluating skill quality
- Generating training data
- Packaging skills for distribution

## Files

- `agents/` - Agent definitions for different evaluation tasks
- `scripts/` - Utility scripts for skill development
- `eval-viewer/` - Evaluation result visualization
- `references/` - Schema definitions and documentation
"#;

        std::fs::write(bundled_path.join("skill-creator").join("SKILL.md"), skill_creator_md)?;
        std::fs::create_dir_all(bundled_path.join("skill-creator").join("agents"))?;
        std::fs::write(bundled_path.join("skill-creator").join("agents").join("analyzer.md"), "# Analyzer\n\nAnalyzes skill structure and quality.")?;
        std::fs::write(bundled_path.join("skill-creator").join("agents").join("grader.md"), "# Grader\n\nGrades skill performance.")?;

        Ok(())
    }

    fn scan_skills_dir(&self, base_path: &Path, source: SkillSource) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        if !base_path.exists() {
            return Ok(skills);
        }

        for entry in WalkDir::new(base_path)
            .max_depth(3)
            .follow_links(false)
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if path.is_file() && path.file_name().map(|n| n == "SKILL.md").unwrap_or(false) {
                if let Some(skill) = self.parse_skill_file(path, &source)? {
                    skills.push(skill);
                }
            }
        }

        Ok(skills)
    }

    fn parse_skill_file(&self, skill_md_path: &Path, source: &SkillSource) -> Result<Option<Skill>> {
        let content = match std::fs::read_to_string(skill_md_path) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        let frontmatter = self.parse_frontmatter(&content);

        let name = frontmatter.name.or_else(|| {
            skill_md_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        });

        let name = match name {
            Some(n) => n,
            None => return Ok(None),
        };

        let skill_dir = skill_md_path.parent();
        let files = skill_dir.map(|d| self.build_file_tree(d)).unwrap_or_default();

        let skill = Skill {
            id: Self::generate_skill_id(skill_md_path),
            name,
            description: frontmatter.description.unwrap_or_default(),
            content: Some(content),
            when_to_use: frontmatter.when_to_use,
            enabled: true,
            source: source.clone(),
            loaded_from: source.as_str().to_string(),
            source_dir: skill_md_path
                .parent()
                .map(|p| p.to_string_lossy().to_string()),
            is_example: false,
            files,
            created_at: None,
        };

        Ok(Some(skill))
    }

    fn parse_frontmatter(&self, content: &str) -> FrontmatterData {
        let mut frontmatter = FrontmatterData {
            name: None,
            description: None,
            when_to_use: None,
            allowed_tools: None,
            model: None,
            user_invokable: None,
        };

        let content_trimmed = content.trim_start();
        if !content_trimmed.starts_with("---") {
            return frontmatter;
        }

        if let Some(end_pos) = content_trimmed[3..].find("---") {
            let yaml_block = &content_trimmed[3..end_pos + 3];
            for line in yaml_block.lines() {
                let line = line.trim();
                if line.is_empty() || line == "---" {
                    continue;
                }
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"').trim_matches('\'');
                    match key {
                        "name" => frontmatter.name = Some(value.to_string()),
                        "description" => frontmatter.description = Some(value.to_string()),
                        "when" | "whenToUse" => frontmatter.when_to_use = Some(value.to_string()),
                        "model" => frontmatter.model = Some(value.to_string()),
                        "userInvocable" | "user_invocable" => {
                            frontmatter.user_invokable = Some(value == "true" || value == "yes");
                        }
                        _ => {}
                    }
                }
            }
        }

        frontmatter
    }

    fn build_file_tree(&self, dir: &Path) -> Vec<SkillFile> {
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();

                if name.starts_with('.') || name == "node_modules" {
                    continue;
                }

                let file_type = if path.is_dir() { "folder" } else { "file" };

                let children = if path.is_dir() {
                    Some(self.build_file_tree(&path))
                } else {
                    None
                };

                files.push(SkillFile {
                    name,
                    file_type: file_type.to_string(),
                    children,
                });
            }
        }

        files.sort_by(|a, b| {
            match (&a.file_type[..], &b.file_type[..]) {
                ("folder", "file") => std::cmp::Ordering::Less,
                ("file", "folder") => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        files
    }

    fn generate_skill_id(skill_path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        skill_path.to_string_lossy().hash(&mut hasher);
        format!("skill_{:x}", hasher.finish())
    }

    pub fn get_skill_content(&self, skill_id: &str, file_path: &str) -> Result<String> {
        let skill_path = self.find_skill_path(skill_id)?;
        let full_path = skill_path.join(file_path);
        let content = std::fs::read_to_string(&full_path)?;
        Ok(content)
    }

    pub fn create_skill(&self, name: &str, description: &str, content: &str) -> Result<Skill> {
        let skill_dir = self.skills_dir.join(name);
        std::fs::create_dir_all(&skill_dir)?;

        let skill_md = format!(
            "---\nname: {}\ndescription: {}\n---\n\n{}",
            name, description, content
        );
        std::fs::write(skill_dir.join("SKILL.md"), &skill_md)?;

        let skill = Skill {
            id: Self::generate_skill_id(&skill_dir.join("SKILL.md")),
            name: name.to_string(),
            description: description.to_string(),
            content: Some(skill_md),
            when_to_use: None,
            enabled: true,
            source: SkillSource::User,
            loaded_from: "user".to_string(),
            source_dir: Some(skill_dir.to_string_lossy().to_string()),
            is_example: false,
            files: vec![SkillFile {
                name: "SKILL.md".to_string(),
                file_type: "file".to_string(),
                children: None,
            }],
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };

        Ok(skill)
    }

    pub fn update_skill(&self, skill_id: &str, updates: HashMap<String, serde_json::Value>) -> Result<()> {
        let skill_path = self.find_skill_path(skill_id)?;
        let skill_md_path = skill_path.join("SKILL.md");

        if !skill_md_path.exists() {
            anyhow::bail!("SKILL.md not found");
        }

        let content = std::fs::read_to_string(&skill_md_path)?;

        let mut new_content = content.clone();
        if let Some(name) = updates.get("name").and_then(|v| v.as_str()) {
            new_content = Self::update_yaml_field(&new_content, "name", name);
        }
        if let Some(description) = updates.get("description").and_then(|v| v.as_str()) {
            new_content = Self::update_yaml_field(&new_content, "description", description);
        }

        std::fs::write(&skill_md_path, new_content)?;
        Ok(())
    }

    fn update_yaml_field(content: &str, field: &str, value: &str) -> String {
        let pattern = format!("^{}:", field);
        if let Some(idx) = content.lines().position(|l| l.trim().starts_with(&pattern)) {
            let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
            lines[idx] = format!("{}: {}", field, value);
            lines.join("\n")
        } else {
            content.to_string()
        }
    }

    fn find_skill_path(&self, skill_id: &str) -> Result<PathBuf> {
        for entry in WalkDir::new(&self.skills_dir)
            .max_depth(2)
            .follow_links(false)
        {
            if let Ok(entry) = entry {
                if entry.path().file_name().map(|n| n == "SKILL.md").unwrap_or(false) {
                    let id = Self::generate_skill_id(entry.path());
                    if id == skill_id {
                        return Ok(entry.path().parent().unwrap().to_path_buf());
                    }
                }
            }
        }
        anyhow::bail!("Skill not found: {}", skill_id)
    }

    pub fn delete_skill(&self, skill_id: &str) -> Result<()> {
        let skill_path = self.find_skill_path(skill_id)?;
        std::fs::remove_dir_all(skill_path)?;
        Ok(())
    }

    pub async fn get_skill_by_id(&self, skill_id: &str) -> Result<Option<Skill>> {
        let skills = self.load_skills().await?;
        Ok(skills.into_iter().find(|s| s.id == skill_id))
    }
}

impl Default for SkillsManager {
    fn default() -> Self {
        Self::new()
    }
}
