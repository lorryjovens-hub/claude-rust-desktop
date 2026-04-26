use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<String>,
    pub extension: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub file: String,
    pub line_number: usize,
    pub content: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStats {
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_size: u64,
    pub by_extension: HashMap<String, usize>,
}

pub struct FileOperations;

impl FileOperations {
    pub fn read_file(path: &str, offset: Option<usize>, limit: Option<usize>) -> Result<String> {
        let content = fs::read_to_string(path)?;
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);

        let lines: Vec<&str> = content.lines().collect();
        if offset >= lines.len() {
            return Ok(String::new());
        }

        let end = (offset + limit).min(lines.len());
        Ok(lines[offset..end].join("\n"))
    }

    pub fn write_file(path: &str, content: &str) -> Result<()> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    pub fn edit_file(path: &str, old_string: &str, new_string: &str, replace_all: bool) -> Result<()> {
        let mut file_content = fs::read_to_string(path)?;

        if replace_all {
            file_content = file_content.replace(old_string, new_string);
        } else {
            if let Some(pos) = file_content.find(old_string) {
                file_content = format!("{}{}{}", &file_content[..pos], new_string, &file_content[pos + old_string.len()..]);
            } else {
                return Err(anyhow!("Could not find the specified string in the file"));
            }
        }

        fs::write(path, file_content)?;
        Ok(())
    }

    pub fn multi_edit_file(path: &str, edits: Vec<(String, String)>) -> Result<()> {
        let mut file_content = fs::read_to_string(path)?;

        for (old_string, new_string) in edits {
            file_content = file_content.replace(&old_string, &new_string);
        }

        fs::write(path, file_content)?;
        Ok(())
    }

    pub fn list_directory(path: &str, recursive: bool) -> Result<Vec<FileEntry>> {
        let mut entries = Vec::new();
        let base_path = Path::new(path);

        if recursive {
            for entry in WalkDir::new(base_path).into_iter().filter_map(|e| e.ok()) {
                let metadata = entry.metadata()?;
                let file_path = entry.path();

                entries.push(FileEntry {
                    path: file_path.to_string_lossy().to_string(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    is_dir: metadata.is_dir(),
                    size: metadata.len(),
                    modified: metadata.modified().ok().map(|t| {
                        chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
                    }),
                    extension: file_path.extension().map(|e| e.to_string_lossy().to_string()),
                });
            }
        } else {
            for entry in fs::read_dir(base_path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                let file_path = entry.path();

                entries.push(FileEntry {
                    path: file_path.to_string_lossy().to_string(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    is_dir: metadata.is_dir(),
                    size: metadata.len(),
                    modified: metadata.modified().ok().map(|t| {
                        chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
                    }),
                    extension: file_path.extension().map(|e| e.to_string_lossy().to_string()),
                });
            }
        }

        entries.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            }
        });

        Ok(entries)
    }

    pub fn glob(pattern: &str, path: Option<&str>) -> Result<Vec<String>> {
        let base = PathBuf::from(path.unwrap_or("."));
        let mut matches = Vec::new();

        for entry in glob::glob(&format!(
            "{}/{}",
            base.to_string_lossy(),
            pattern
        ))? {
            if let Ok(path) = entry {
                matches.push(path.to_string_lossy().to_string());
            }
        }

        matches.sort();
        Ok(matches)
    }

    pub fn grep(pattern: &str, path: &str, include: Option<&str>, context: Option<usize>) -> Result<Vec<SearchResult>> {
        let re = regex::Regex::new(pattern)?;
        let context_lines = context.unwrap_or(0);
        let mut results = Vec::new();

        let paths: Vec<PathBuf> = if Path::new(path).is_file() {
            vec![PathBuf::from(path)]
        } else if let Some(glob_pattern) = include {
            let full_pattern = format!("{}/{}", path, glob_pattern);
            glob::glob(&full_pattern)?
                .filter_map(|p| p.ok())
                .collect()
        } else {
            WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.path().to_path_buf())
                .collect()
        };

        for file_path in paths {
            if let Ok(content) = fs::read_to_string(&file_path) {
                let lines: Vec<&str> = content.lines().collect();

                for (idx, line) in lines.iter().enumerate() {
                    if re.is_match(line) {
                        let context_before: Vec<String> = lines[..idx]
                            .iter()
                            .rev()
                            .take(context_lines)
                            .map(|s| s.to_string())
                            .collect();

                        let context_after: Vec<String> = lines[idx + 1..]
                            .iter()
                            .take(context_lines)
                            .map(|s| s.to_string())
                            .collect();

                        results.push(SearchResult {
                            file: file_path.to_string_lossy().to_string(),
                            line_number: idx + 1,
                            content: line.to_string(),
                            context_before: context_before.into_iter().rev().collect(),
                            context_after,
                        });
                    }
                }
            }
        }

        Ok(results)
    }

    pub fn get_file_stats(path: &str) -> Result<FileStats> {
        let mut total_files = 0;
        let mut total_dirs = 0;
        let mut total_size = 0u64;
        let mut by_extension: HashMap<String, usize> = HashMap::new();

        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                total_files += 1;
                total_size += metadata.len();

                if let Some(ext) = entry.path().extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    *by_extension.entry(ext_str).or_insert(0) += 1;
                }
            } else if metadata.is_dir() {
                total_dirs += 1;
            }
        }

        Ok(FileStats {
            total_files,
            total_dirs,
            total_size,
            by_extension,
        })
    }

    pub fn create_directory(path: &str) -> Result<()> {
        fs::create_dir_all(path)?;
        Ok(())
    }

    pub fn delete_file(path: &str) -> Result<()> {
        let p = Path::new(path);
        if p.is_dir() {
            fs::remove_dir_all(p)?;
        } else {
            fs::remove_file(p)?;
        }
        Ok(())
    }

    pub fn copy_file(from: &str, to: &str) -> Result<()> {
        if let Some(parent) = Path::new(to).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
        Ok(())
    }

    pub fn move_file(from: &str, to: &str) -> Result<()> {
        if let Some(parent) = Path::new(to).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(from, to)?;
        Ok(())
    }

    pub fn exists(path: &str) -> bool {
        Path::new(path).exists()
    }

    pub fn is_directory(path: &str) -> bool {
        Path::new(path).is_dir()
    }

    pub fn file_size(path: &str) -> Result<u64> {
        let metadata = fs::metadata(path)?;
        Ok(metadata.len())
    }

    pub fn get_modified_time(path: &str) -> Result<String> {
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;
        Ok(chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339())
    }

    pub fn get_extension(path: &str) -> Option<String> {
        Path::new(path)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
    }

    pub fn read_binary(path: &str) -> Result<Vec<u8>> {
        Ok(fs::read(path)?)
    }

    pub fn write_binary(path: &str, data: &[u8]) -> Result<()> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, data)?;
        Ok(())
    }

    pub fn get_relative_path(path: &str, base: &str) -> Result<String> {
        let p = Path::new(path);
        let b = Path::new(base);

        Ok(p.strip_prefix(b)?
            .to_string_lossy()
            .to_string())
    }

    pub fn get_absolute_path(path: &str) -> Result<String> {
        let p = Path::new(path);
        if p.is_absolute() {
            Ok(p.to_string_lossy().to_string())
        } else {
            Ok(p.canonicalize()?.to_string_lossy().to_string())
        }
    }

    pub fn find_files_by_extension(dir: &str, extension: &str) -> Result<Vec<String>> {
        let pattern = format!("{}/**/*.{}", dir, extension);
        let mut results = Vec::new();

        for entry in glob::glob(&pattern)? {
            if let Ok(path) = entry {
                results.push(path.to_string_lossy().to_string());
            }
        }

        Ok(results)
    }

    pub fn get_directory_size(path: &str) -> Result<u64> {
        let mut total_size = 0u64;

        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                total_size += entry.metadata()?.len();
            }
        }

        Ok(total_size)
    }

    pub fn get_line_count(path: &str) -> Result<usize> {
        let content = fs::read_to_string(path)?;
        Ok(content.lines().count())
    }

    pub fn get_language_from_extension(path: &str) -> Option<String> {
        let ext = Path::new(path).extension()?.to_str()?.to_lowercase();

        let languages: HashMap<&str, &str> = [
            ("rs", "Rust"),
            ("js", "JavaScript"),
            ("ts", "TypeScript"),
            ("jsx", "JavaScript (JSX)"),
            ("tsx", "TypeScript (TSX)"),
            ("py", "Python"),
            ("rb", "Ruby"),
            ("go", "Go"),
            ("java", "Java"),
            ("c", "C"),
            ("cpp", "C++"),
            ("h", "C Header"),
            ("hpp", "C++ Header"),
            ("cs", "C#"),
            ("swift", "Swift"),
            ("kt", "Kotlin"),
            ("scala", "Scala"),
            ("php", "PHP"),
            ("html", "HTML"),
            ("css", "CSS"),
            ("scss", "SCSS"),
            ("sass", "Sass"),
            ("less", "Less"),
            ("json", "JSON"),
            ("xml", "XML"),
            ("yaml", "YAML"),
            ("yml", "YAML"),
            ("toml", "TOML"),
            ("md", "Markdown"),
            ("sql", "SQL"),
            ("sh", "Shell"),
            ("bash", "Bash"),
            ("zsh", "Zsh"),
            ("ps1", "PowerShell"),
            ("dockerfile", "Dockerfile"),
            ("makefile", "Makefile"),
            ("cmake", "CMake"),
        ].iter().cloned().collect();

        languages.get(ext.as_str()).map(|s| s.to_string())
    }
}
