//! Enhanced UI Module - Advanced terminal UI with animations and effects
//!
//! Features:
//! - Smooth animations
//! - Progress bars
//! - Syntax highlighting
//! - Better markdown rendering
//! - Interactive menus

use colored::{ColoredString, Colorize};
use std::io::{self, Write};
use std::time::Duration;
use std::thread;

/// Animation frame for spinner
pub struct Spinner {
    frames: Vec<&'static str>,
    current: usize,
    message: String,
}

impl Spinner {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            frames: vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            current: 0,
            message: message.into(),
        }
    }

    pub fn tick(&mut self) {
        let frame = self.frames[self.current % self.frames.len()];
        print!("\r  {} {} ",
            frame.truecolor(147, 112, 219).bold(),
            self.message.truecolor(180, 180, 180)
        );
        io::stdout().flush().ok();
        self.current += 1;
    }

    pub fn finish(&self, success: bool) {
        let icon = if success { "✓".green() } else { "✗".red() };
        println!("\r  {} {} ", icon, self.message);
    }
}

/// Progress bar for long operations
pub struct ProgressBar {
    total: usize,
    current: usize,
    width: usize,
    message: String,
}

impl ProgressBar {
    pub fn new(total: usize, message: impl Into<String>) -> Self {
        Self {
            total,
            current: 0,
            width: 40,
            message: message.into(),
        }
    }

    pub fn update(&mut self, current: usize) {
        self.current = current;
        let percent = (self.current as f32 / self.total as f32 * 100.0) as usize;
        let filled = (self.current as f32 / self.total as f32 * self.width as f32) as usize;
        let empty = self.width - filled;

        print!("\r  {} [{}{}] {}%",
            self.message.truecolor(147, 112, 219),
            "█".repeat(filled).truecolor(147, 112, 219),
            "░".repeat(empty).truecolor(80, 80, 80),
            percent.to_string().truecolor(255, 140, 66).bold()
        );
        io::stdout().flush().ok();
    }

    pub fn increment(&mut self) {
        self.update(self.current + 1);
    }

    pub fn finish(&self) {
        self.update(self.total);
        println!();
    }
}

/// Animated typing effect with variable speed
pub fn typewrite(text: &str, base_delay_ms: u64, variance_ms: u64) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    for c in text.chars() {
        print!("{}", c);
        io::stdout().flush().ok();

        let delay = base_delay_ms + rng.gen_range(0..variance_ms);
        thread::sleep(Duration::from_millis(delay));
    }
}

/// Print a code block with syntax highlighting simulation
pub fn print_code_block(code: &str, language: Option<&str>) {
    let lang = language.unwrap_or("");

    println!();
    println!("  {} {} {}",
        "┌─".truecolor(80, 80, 80),
        lang.truecolor(147, 112, 219).bold(),
        "─".repeat(50).truecolor(80, 80, 80)
    );

    for line in code.lines() {
        let highlighted = highlight_code(line);
        println!("  {} {}", "│".truecolor(80, 80, 80), highlighted);
    }

    println!("  {}", "└".repeat(52).truecolor(80, 80, 80));
    println!();
}

/// Simple syntax highlighting for code
fn highlight_code(line: &str) -> ColoredString {
    // Keywords
    let keywords = ["fn", "let", "mut", "use", "pub", "struct", "impl", "if", "else", "return", "match", "async", "await", "for", "while", "loop"];
    for kw in &keywords {
        if line.trim().starts_with(kw) || line.contains(&format!(" {} ", kw)) {
            return line.truecolor(180, 140, 250);
        }
    }

    // Comments
    if line.trim().starts_with("//") || line.trim().starts_with("#") {
        return line.bright_black();
    }

    // Strings
    if line.contains('"') || line.contains('\'') {
        return line.truecolor(140, 220, 140);
    }

    // Numbers
    if line.chars().any(|c| c.is_ascii_digit()) {
        return line.truecolor(255, 180, 100);
    }

    line.normal()
}

/// Print a fancy box with title
pub fn print_box(title: &str, content: &[&str]) {
    let max_width = content.iter().map(|s| s.len()).max().unwrap_or(0).max(title.len());
    let width = max_width + 4;

    println!();
    println!("  {} {} {}",
        "╭".truecolor(147, 112, 219),
        title.truecolor(147, 112, 219).bold(),
        "─".repeat(width - title.len() - 2).truecolor(147, 112, 219)
    );

    for line in content {
        println!("  {} {:width$} {}",
            "│".truecolor(147, 112, 219),
            line.bright_white(),
            "│".truecolor(147, 112, 219),
            width = width - 2
        );
    }

    println!("  {}", "╰".repeat(width + 2).truecolor(147, 112, 219));
    println!();
}

/// Print a tree structure
pub fn print_tree(items: &[(String, Vec<String>)]) {
    println!();
    for (i, (root, children)) in items.iter().enumerate() {
        let is_last = i == items.len() - 1;
        let branch = if is_last { "└──" } else { "├──" };

        println!("  {} {}",
            branch.truecolor(147, 112, 219),
            root.bright_cyan().bold()
        );

        for (j, child) in children.iter().enumerate() {
            let is_last_child = j == children.len() - 1;
            let child_branch = if is_last_child { "    └──" } else { "    ├──" };

            println!("  {} {}",
                child_branch.truecolor(100, 100, 100),
                child.bright_white()
            );
        }
    }
    println!();
}

/// Print a table with headers and rows
pub fn print_table(headers: &[&str], rows: &[(String, String, String)]) {
    if rows.is_empty() {
        println!("  (no data)");
        return;
    }

    let col1_width = rows.iter().map(|r| r.0.len()).max().unwrap_or(0).max(headers[0].len()) + 2;
    let col2_width = rows.iter().map(|r| r.1.len()).max().unwrap_or(0).max(headers[1].len()) + 2;
    let col3_width = rows.iter().map(|r| r.2.len()).max().unwrap_or(0).max(headers[2].len()) + 2;

    // Print header
    println!();
    println!("  {} {} {} {}",
        headers[0].truecolor(147, 112, 219).bold(),
        " ".repeat(col1_width - headers[0].len()),
        headers[1].truecolor(147, 112, 219).bold(),
        " ".repeat(col2_width - headers[1].len()),
    );
    print!("  {}{}{}",
        "─".repeat(col1_width).truecolor(80, 80, 80),
        "─".repeat(col2_width).truecolor(80, 80, 80),
        "─".repeat(col3_width).truecolor(80, 80, 80)
    );
    println!();

    // Print rows
    for (col1, col2, col3) in rows {
        println!("  {} {} {} {} {}",
            col1.bright_cyan(),
            " ".repeat(col1_width - col1.len()),
            col2.bright_white(),
            " ".repeat(col2_width - col2.len()),
            col3.truecolor(150, 150, 150)
        );
    }
    println!();
}

/// Print a diff view
pub fn print_diff(old: &str, new: &str) {
    use similar::{ChangeTag, TextDiff};

    println!();
    let diff = TextDiff::from_lines(old, new);

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => {
                print!("  {} {}",
                    "-".red(),
                    change.value().to_string().red()
                );
            }
            ChangeTag::Insert => {
                print!("  {} {}",
                    "+".green(),
                    change.value().to_string().green()
                );
            }
            ChangeTag::Equal => {
                print!("  {} {}",
                    " ".bright_black(),
                    change.value().to_string().bright_black()
                );
            }
        }
    }
    println!();
}

/// Print an interactive menu and return selected index
pub fn interactive_menu(items: &[&str], prompt: &str) -> Option<usize> {
    println!();
    println!("  {}", prompt.truecolor(147, 112, 219).bold());
    println!();

    for (i, item) in items.iter().enumerate() {
        println!("  {} {} {}",
            "▸".truecolor(100, 100, 100),
            (i + 1).to_string().truecolor(255, 140, 66).bold(),
            item.bright_white()
        );
    }
    println!();

    print!("  {} ", "▸".truecolor(255, 140, 66));
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok()?;

    input.trim().parse::<usize>().ok().map(|n| n.saturating_sub(1))
}

/// Print a notification banner
pub fn print_notification(message: &str, notification_type: NotificationType) {
    let (icon, color) = match notification_type {
        NotificationType::Success => ("✓", Color::Green),
        NotificationType::Error => ("✗", Color::Red),
        NotificationType::Warning => ("⚠", Color::Yellow),
        NotificationType::Info => ("ℹ", Color::Cyan),
    };

    println!();
    println!("  {} {}",
        icon.color(color).bold(),
        message.color(color)
    );
    println!();
}

#[derive(Debug, Clone, Copy)]
pub enum NotificationType {
    Success,
    Error,
    Warning,
    Info,
}

use colored::Color;
