//! CLI Module - Command line interface
//!
//! Provides CLI-specific functionality: input handling, output rendering,
//! initialization, and main entry point orchestration.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub verbose: bool,
    pub quiet: bool,
    pub output_format: OutputFormat,
    pub no_color: bool,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            quiet: false,
            output_format: OutputFormat::Pretty,
            no_color: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OutputFormat {
    Pretty,
    Compact,
    Json,
    Markdown,
}

pub mod input {
    use std::io::{self, BufRead, Write};

    pub struct InputReader {
        reader: io::BufReader<io::Stdin>,
    }

    impl Default for InputReader {
        fn default() -> Self {
            Self::new()
        }
    }

    impl InputReader {
        pub fn new() -> Self {
            Self {
                reader: io::BufReader::new(io::stdin()),
            }
        }

        pub fn read_line(&mut self, prompt: &str) -> io::Result<String> {
            print!("{}", prompt);
            io::stdout().flush()?;
            let mut line = String::new();
            self.reader.read_line(&mut line)?;
            Ok(line.trim_end().to_string())
        }

        pub fn read_multiline(&mut self, terminator: &str) -> io::Result<String> {
            let mut input = String::new();
            loop {
                let mut line = String::new();
                self.reader.read_line(&mut line)?;
                if line.trim_end() == terminator {
                    break;
                }
                input.push_str(&line);
            }
            Ok(input)
        }
    }
}

pub mod render {
    use super::OutputFormat;

    pub struct Renderer {
        format: OutputFormat,
        no_color: bool,
    }

    impl Default for Renderer {
        fn default() -> Self {
            Self::new(OutputFormat::Pretty, false)
        }
    }

    impl Renderer {
        pub fn new(format: OutputFormat, no_color: bool) -> Self {
            Self { format, no_color }
        }

        pub fn render(&self, content: &str) -> String {
            match self.format {
                OutputFormat::Json => self.render_json(content),
                OutputFormat::Compact | OutputFormat::Markdown => content.to_string(),
                _ => self.render_pretty(content),
            }
        }

        fn render_pretty(&self, content: &str) -> String {
            if self.no_color {
                content.to_string()
            } else {
                format!("\x1b[1;34m{}\x1b[0m", content)
            }
        }

        fn render_json(&self, content: &str) -> String {
            serde_json::to_string_pretty(&serde_json::json!({
                "output": content
            })).unwrap_or_else(|_| content.to_string())
        }
    }
}

pub mod init {
    use std::path::Path;

    pub fn ensure_directories(base_path: &Path) -> std::io::Result<()> {
        let dirs = [
            ".claude",
            ".claude/sessions",
            ".claude/logs",
            ".claude/plugins",
        ];

        for dir in &dirs {
            let path = base_path.join(dir);
            if !path.exists() {
                std::fs::create_dir_all(&path)?;
            }
        }

        Ok(())
    }
}