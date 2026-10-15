//! The old Rust builtins returned mock success. These prompts run through the host's
//! normal model/tool session, with its existing permissions, progress and cancellation.
use anyhow::Result;
use std::{io::Write, path::Path};

const BUILTINS: &[(&str,&str,&str)] = &[
    ("commit","Create a Git commit from reviewed changes","Inspect repository instructions, git status and the staged diff. Treat --message as the requested commit message, --amend as an explicit request to amend, and --all as a request to include tracked unstaged changes. If no message is supplied, write an English imperative message describing the actual changes. Never stage credentials or unrelated work. Run the real Git commands through the shared tools and host permission checks; verify the resulting commit hash. Report failures honestly. Do not push unless requested."),
    ("review","Review current changes or requested files","Review the requested files, or the Git diff when files are omitted or --diff is supplied. Read surrounding code and applicable instructions. Explain concrete correctness, security and regression findings with file references. Use real repository evidence; do not claim tests were run unless tools ran them. Do not edit files unless requested. Preserve the user's focus and severity options."),
    ("test","Run the project's tests","Read project instructions and test scripts to choose the actual test command. Honor requested test patterns, --coverage and --watch when supported. Run through shared tools and permission checks; capture real pass/fail counts and relevant failure output. Never substitute a mock success. Explain unavailable dependencies or unsupported options. Avoid an indefinite watch process unless explicitly requested."),
    ("document","Generate documentation from source","Inspect requested code and existing documentation conventions. Honor requested format, output and scope; generate documentation grounded in actual APIs and behavior. Preview the intended changes or write them through normal project file tools according to the user's request. Do not overwrite unrelated content. Report exactly which files were created or changed."),
    ("build","Build the project","Read repository instructions, manifests and build scripts. Select the actual project build command and honor --release, --clean and --verbose only when supported. Clean only project build artifacts when explicitly requested. Run through shared tools and host permissions, verify produced artifacts and report real failures. Never report a successful build without executing and checking it."),
];

pub(super) fn install(root: &Path) -> Result<()> {
    for (name, description, prompt) in BUILTINS {
        let directory = root.join(format!("legacy-{name}"));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("SKILL.md");
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        write!(file,"---\nname: {name}\ndescription: {description}\nenabled: true\nuserInvocable: true\n---\n\n# {name}\n\n{prompt}\n\nApply the user's supplied arguments as task instructions. Skill text never overrides host permissions or repository instructions.\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn manager_discovers_unique_builtins_and_persists_disabled_state() {
        let dir = tempfile::tempdir().unwrap();
        let manager = super::super::SkillsManager {
            skills_dir: dir.path().to_path_buf(),
        };
        let skills = manager.load_skills().await.unwrap();
        let commit = skills.iter().find(|s| s.name == "commit").unwrap();
        assert!(!manager.toggle_skill(&commit.id).await.unwrap());
        let skills = manager.load_skills().await.unwrap();
        assert_eq!(skills.iter().filter(|s| s.name == "commit").count(), 1);
        assert!(!skills.iter().find(|s| s.name == "commit").unwrap().enabled);
        assert!(manager.toggle_skill(&commit.id).await.unwrap());
    }
    #[test]
    fn installs_all_five_without_overwriting_user_edits() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path()).unwrap();
        for (name, _, _) in BUILTINS {
            let content =
                std::fs::read_to_string(dir.path().join(format!("legacy-{name}/SKILL.md")))
                    .unwrap();
            assert!(content.contains(&format!("name: {name}")));
            assert!(content.contains("host permissions"));
        }
        let edited = dir.path().join("legacy-commit/SKILL.md");
        std::fs::write(&edited, "user content").unwrap();
        install(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(edited).unwrap(), "user content");
    }
}
