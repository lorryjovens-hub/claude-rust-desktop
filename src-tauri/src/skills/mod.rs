use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub triggers: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMatch {
    pub skill_name: String,
    pub confidence: f32,
    pub trigger_reason: String,
}

pub struct SkillManager {
    skills_dir: PathBuf,
    skills: HashMap<String, Skill>,
}

impl SkillManager {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills_dir,
            skills: HashMap::new(),
        }
    }

    pub fn load_skills(&mut self) -> Result<()> {
        if !self.skills_dir.exists() {
            std::fs::create_dir_all(&self.skills_dir)?;
        }

        self.skills.clear();

        let entries = std::fs::read_dir(&self.skills_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    if let Ok(skill) = Self::load_skill_from_file(&skill_md) {
                        self.skills.insert(skill.name.clone(), skill);
                    }
                }
            }
        }

        Ok(())
    }

    fn load_skill_from_file(path: &Path) -> Result<Skill> {
        let content = std::fs::read_to_string(path)?;

        let name = Self::extract_yaml_field(&content, "name")
            .ok_or_else(|| anyhow!("Missing 'name' in SKILL.md"))?;

        let description = Self::extract_yaml_field(&content, "description")
            .unwrap_or_default();

        let instructions = Self::extract_markdown_content(&content)?;
        let triggers = Self::generate_triggers(&name, &description);

        Ok(Skill {
            name,
            description,
            instructions,
            triggers,
            enabled: true,
        })
    }

    fn extract_yaml_field(content: &str, field: &str) -> Option<String> {
        let pattern = format!(r#"{}: "?([^"\n]+)"?"#, field);
        let re = regex::Regex::new(&pattern).ok()?;
        let caps = re.captures(content)?;
        caps.get(1).map(|m| m.as_str().trim().to_string())
    }

    fn extract_markdown_content(content: &str) -> Result<String> {
        let mut in_frontmatter = false;
        let mut started = false;
        let mut lines = Vec::new();

        for line in content.lines() {
            if line.starts_with("---") {
                if !started {
                    in_frontmatter = !in_frontmatter;
                    started = true;
                    continue;
                }
            }
            if !in_frontmatter {
                lines.push(line);
            }
        }

        Ok(lines.join("\n").trim().to_string())
    }

    fn generate_triggers(name: &str, description: &str) -> Vec<String> {
        let mut triggers = Vec::new();

        let name_lower = name.to_lowercase();
        triggers.push(name_lower.clone());
        triggers.push(name_lower.replace('-', " "));
        triggers.push(name_lower.replace('_', " "));

        let desc_words: Vec<&str> = description.split_whitespace().take(10).collect();
        if !desc_words.is_empty() {
            triggers.push(desc_words.join(" "));
        }

        triggers
    }

    pub fn find_matching_skill(&self, input: &str) -> Option<SkillMatch> {
        let input_lower = input.to_lowercase();

        let mut best_match: Option<SkillMatch> = None;

        for (name, skill) in &self.skills {
            if !skill.enabled {
                continue;
            }

            let mut confidence = 0.0;
            let mut trigger_reason = String::new();

            for trigger in &skill.triggers {
                let trigger_lower = trigger.to_lowercase();
                if input_lower.contains(&trigger_lower) {
                    let weight = trigger_lower.len() as f32 / input_lower.len() as f32;
                    if weight > confidence {
                        confidence = weight;
                        trigger_reason = format!("matched trigger: {}", trigger);
                    }
                }
            }

            if name.to_lowercase().contains(&input_lower) {
                confidence = confidence.max(0.7);
                if trigger_reason.is_empty() {
                    trigger_reason = "name similarity".to_string();
                }
            }

            if confidence > 0.3 {
                if best_match.as_ref().map(|m| m.confidence).unwrap_or(0.0) < confidence {
                    best_match = Some(SkillMatch {
                        skill_name: name.clone(),
                        confidence,
                        trigger_reason,
                    });
                }
            }
        }

        best_match
    }

    pub fn get_skill(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn list_skills(&self) -> Vec<&Skill> {
        self.skills.values().filter(|s| s.enabled).collect()
    }

    pub fn add_skill(&mut self, skill: Skill) -> Result<()> {
        let skill_dir = self.skills_dir.join(&skill.name);
        std::fs::create_dir_all(&skill_dir)?;

        let yaml_frontmatter = format!(
            "---\nname: {}\ndescription: {}\n---\n",
            skill.name,
            skill.description
        );

        let content = format!("{}\n\n{}", yaml_frontmatter, skill.instructions);
        std::fs::write(skill_dir.join("SKILL.md"), content)?;

        self.skills.insert(skill.name.clone(), skill);
        Ok(())
    }

    pub fn update_skill(&mut self, name: &str, skill: Skill) -> Result<()> {
        if !self.skills.contains_key(name) {
            return Err(anyhow!("Skill '{}' not found", name));
        }

        let skill_dir = self.skills_dir.join(name);
        if !skill_dir.exists() {
            std::fs::create_dir_all(&skill_dir)?;
        }

        let yaml_frontmatter = format!(
            "---\nname: {}\ndescription: {}\n---\n",
            skill.name,
            skill.description
        );

        let content = format!("{}\n\n{}", yaml_frontmatter, skill.instructions);
        std::fs::write(skill_dir.join("SKILL.md"), content)?;

        self.skills.insert(name.to_string(), skill);
        Ok(())
    }

    pub fn delete_skill(&mut self, name: &str) -> Result<()> {
        if let Some(mut skill) = self.skills.remove(name) {
            skill.enabled = false;
        }

        let skill_dir = self.skills_dir.join(name);
        if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)?;
        }

        Ok(())
    }

    pub fn enable_skill(&mut self, name: &str, enabled: bool) -> Result<()> {
        if let Some(skill) = self.skills.get_mut(name) {
            skill.enabled = enabled;
            let skill_dir = self.skills_dir.join(name);
            if !skill_dir.exists() {
                std::fs::create_dir_all(&skill_dir)?;
            }

            let yaml_frontmatter = format!(
                "---\nname: {}\ndescription: {}\n---\n",
                skill.name,
                skill.description
            );
            let content = format!("{}\n\n{}", yaml_frontmatter, skill.instructions);
            std::fs::write(skill_dir.join("SKILL.md"), content)?;
        }

        Ok(())
    }

    pub fn get_skill_prompt(&self, name: &str) -> Option<String> {
        self.skills.get(name).map(|s| s.instructions.clone())
    }

    pub fn search_skills(&self, query: &str) -> Vec<SkillMatch> {
        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        for (name, skill) in &self.skills {
            if !skill.enabled {
                continue;
            }

            let mut confidence = 0.0;

            if name.to_lowercase().contains(&query_lower) {
                confidence = 0.8;
            } else if skill.description.to_lowercase().contains(&query_lower) {
                confidence = 0.6;
            } else if skill.instructions.to_lowercase().contains(&query_lower) {
                confidence = 0.4;
            }

            if confidence > 0.3 {
                matches.push(SkillMatch {
                    skill_name: name.clone(),
                    confidence,
                    trigger_reason: "search match".to_string(),
                });
            }
        }

        matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        matches
    }
}

pub struct DefaultSkills;

impl DefaultSkills {
    pub fn get_code_review_skill() -> Skill {
        Skill {
            name: "code-review".to_string(),
            description: "Review code for bugs, performance issues, security vulnerabilities, and suggest improvements".to_string(),
            instructions: r#"# Code Review Skill

You are an expert code reviewer. Analyze the provided code and give constructive feedback.

## Review Areas

1. **Correctness** - Does the code do what it's supposed to do?
2. **Security** - Are there any security vulnerabilities?
3. **Performance** - Could the code be more efficient?
4. **Readability** - Is the code easy to understand?
5. **Best Practices** - Does it follow language/framework conventions?

## Output Format

Provide your review in markdown with:
- Summary of findings
- Specific issues (with line references if possible)
- Suggestions for improvement
- Positive aspects

## Guidelines

- Be constructive and respectful
- Focus on significant issues, not style preferences
- Explain WHY something is an issue, not just WHAT
- Suggest concrete solutions, not just criticism
"#.to_string(),
            triggers: vec!["code review".to_string(), "review code".to_string(), "check for bugs".to_string()],
            enabled: true,
        }
    }

    pub fn get_doc_writer_skill() -> Skill {
        Skill {
            name: "doc-writer".to_string(),
            description: "Write or update documentation for code, APIs, or projects".to_string(),
            instructions: r#"# Documentation Writer Skill

You are a technical documentation expert. Create clear, concise documentation.

## Types of Documentation

1. **README** - Project overview, setup, usage
2. **API Docs** - Endpoint descriptions, parameters, examples
3. **Code Comments** - Inline explanations for complex logic
4. **Guides** - Tutorials and how-to documents

## Guidelines

- Use clear, simple language
- Include code examples
- Explain WHY, not just WHAT
- Keep docs up-to-date with code
- Use consistent formatting

## Output

Provide well-structured markdown documentation.
"#.to_string(),
            triggers: vec!["write docs".to_string(), "documentation".to_string(), "write readme".to_string()],
            enabled: true,
        }
    }

    pub fn get_frontend_design_skill() -> Skill {
        Skill {
            name: "frontend-design".to_string(),
            description: "Create frontend UI components, pages, or designs using React, HTML/CSS, and modern frameworks".to_string(),
            instructions: r#"# Frontend Design Skill

You are a frontend design expert. Create beautiful, functional user interfaces.

## Capabilities

1. React components with hooks
2. HTML/CSS layouts
3. Tailwind CSS styling
4. Responsive design
5. Interactive UI elements

## Guidelines

- Use modern design patterns
- Ensure accessibility
- Follow DRY principles
- Optimize for performance
- Use semantic HTML

## Output

Provide complete, working code with:
- Component structure
- Styling (inline or Tailwind)
- State management
- Event handlers
"#.to_string(),
            triggers: vec!["create UI".to_string(), "design page".to_string(), "build component".to_string(), "frontend".to_string()],
            enabled: true,
        }
    }

    pub fn get_create_project_skill() -> Skill {
        Skill {
            name: "create-project".to_string(),
            description: "Set up new programming projects with proper structure, configuration, and initial files".to_string(),
            instructions: r#"# Project Creator Skill

You help users create new programming projects from scratch.

## Project Setup Steps

1. **Understand Requirements** - What type of project? What language/framework?
2. **Create Structure** - Set up directories and files
3. **Configuration** - Package.json, Cargo.toml, etc.
4. **Initial Files** - Main entry point, basic modules
5. **Documentation** - README with setup instructions

## Output

Create all necessary files with proper:
- Directory structure
- Configuration files
- Basic working code
- Setup instructions
"#.to_string(),
            triggers: vec!["new project".to_string(), "create project".to_string(), "setup project".to_string(), "scaffold".to_string()],
            enabled: true,
        }
    }

    pub fn get_all_default_skills() -> Vec<Skill> {
        vec![
            Self::get_code_review_skill(),
            Self::get_doc_writer_skill(),
            Self::get_frontend_design_skill(),
            Self::get_create_project_skill(),
        ]
    }
}
