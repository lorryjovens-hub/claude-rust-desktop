use std::{fs, io, path::Path};

/// Windows PowerShell 5.1 needs a BOM to recognize UTF-8 script source with -File.
pub(super) fn write_utf8(path: &Path, content: &str) -> io::Result<usize> {
    let powershell_script = cfg!(windows)
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("ps1"))
            .unwrap_or(false);
    let encoded = if powershell_script && !content.starts_with('\u{feff}') {
        format!("\u{feff}{content}")
    } else {
        content.to_string()
    };
    fs::write(path, encoded.as_bytes())?;
    Ok(encoded.len())
}

/// File cmdlets and redirected console output must agree on UTF-8.
pub(super) fn powershell_command(command: &str) -> String {
    format!("$OutputEncoding = [System.Text.UTF8Encoding]::new($false); [Console]::InputEncoding = $OutputEncoding; [Console]::OutputEncoding = $OutputEncoding; $PSDefaultParameterValues['*:Encoding'] = 'utf8';\n{command}")
}

pub(super) fn cmd_command(command: &str) -> String {
    format!("chcp 65001 >nul & {command}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    #[test]
    fn ordinary_files_remain_utf8_without_a_bom() {
        let path = std::env::temp_dir().join(format!("claude-utf8-{}.txt", std::process::id()));
        let text = "提交使用率\n";
        assert_eq!(write_utf8(&path, text).unwrap(), text.len());
        assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
        fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_powershell_reads_chinese_scripts_and_file_content_as_utf8() {
        let directory =
            std::env::temp_dir().join(format!("claude powershell utf8 {}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let script = directory.join("脚本.PS1");
        let data = directory.join("输入.txt");
        write_utf8(&data, "物理内存").unwrap();
        let source = "param([string]$InputPath)\n'提交使用率'\nGet-Content -LiteralPath $InputPath -Encoding UTF8\n[Console]::Error.WriteLine('错误输出')";
        write_utf8(&script, source).unwrap();
        let once = fs::read(&script).unwrap();
        assert!(once.starts_with(&[0xef, 0xbb, 0xbf]));
        write_utf8(&script, &fs::read_to_string(&script).unwrap()).unwrap();
        assert_eq!(fs::read(&script).unwrap(), once);
        let command =
            powershell_command("& $args[0] -InputPath $args[1]; Get-Content -LiteralPath $args[1]");
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
            ])
            .arg(
                command
                    .replace("$args[0]", &format!("'{}'", script.display()))
                    .replace("$args[1]", &format!("'{}'", data.display())),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .replace("\r\n", "\n"),
            "提交使用率\n物理内存\n物理内存\n"
        );
        assert_eq!(String::from_utf8(output.stderr).unwrap().trim(), "错误输出");
        // Cover the cmd -> powershell -File path used by existing conversations.
        let output = std::process::Command::new("cmd.exe")
            .args(["/D", "/S", "/C"])
            .raw_arg(format!("\"{}\"", cmd_command(&format!("powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" -InputPath \"{}\"", script.display(), data.display()))))
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // -File uses the BOM for source and explicit UTF8 for the data file.
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .replace("\r\n", "\n"),
            "提交使用率\n物理内存\n"
        );
        assert_eq!(String::from_utf8(output.stderr).unwrap().trim(), "错误输出");
        fs::remove_file(script).unwrap();
        fs::remove_file(data).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
