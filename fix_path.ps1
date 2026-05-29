$file = "src-tauri/src/tools/mod.rs"
$content = Get-Content $file -Raw

$old = "fn resolve_path(file_path: &str, cwd: &str) -> String {" + "`r`n" +
"    if Path::new(file_path).is_absolute() {" + "`r`n" +
"        file_path.to_string()" + "`r`n" +
"    } else {" + "`r`n" +
"        Path::new(cwd).join(file_path).to_string_lossy().to_string()" + "`r`n" +
"    }" + "`r`n" +
"}"

$new = "fn is_path_allowed(resolved_path: &Path, cwd: &str) -> bool {" + "`r`n" +
"    let canonical = match resolved_path.canonicalize() {" + "`r`n" +
"        Ok(p) => p," + "`r`n" +
"        Err(_) => {" + "`r`n" +
"            let mut current = resolved_path.to_path_buf();" + "`r`n" +
"            let mut suffix = PathBuf::new();" + "`r`n" +
"            loop {" + "`r`n" +
"                if let Ok(p) = current.canonicalize() { break p.join(suffix); }" + "`r`n" +
"                match current.file_name() {" + "`r`n" +
"                    Some(name) => { suffix = PathBuf::from(name).join(suffix); current = match current.parent() { Some(p) => p.to_path_buf(), None => return false }; }" + "`r`n" +
"                    None => return false," + "`r`n" +
"                }" + "`r`n" +
"            }" + "`r`n" +
"        }" + "`r`n" +
"    };" + "`r`n" +
"    if let Ok(cwd_canonical) = Path::new(cwd).canonicalize() {" + "`r`n" +
"        if canonical.starts_with(&cwd_canonical) { return true; }" + "`r`n" +
"    }" + "`r`n" +
"    if let Some(appdata) = dirs::data_dir() {" + "`r`n" +
"        if canonical.starts_with(&appdata) { return true; }" + "`r`n" +
"    }" + "`r`n" +
"    if let Some(local_appdata) = dirs::data_local_dir() {" + "`r`n" +
"        if canonical.starts_with(&local_appdata) { return true; }" + "`r`n" +
"    }" + "`r`n" +
"    false" + "`r`n" +
"}" + "`r`n" +
"" + "`r`n" +
"fn resolve_path(file_path: &str, cwd: &str) -> Result<String> {" + "`r`n" +
"    let resolved = if Path::new(file_path).is_absolute() { PathBuf::from(file_path) } else { Path::new(cwd).join(file_path) };" + "`r`n" +
"    if !is_path_allowed(&resolved, cwd) {" + "`r`n" +
'        return Err(anyhow!("Path is outside allowed directories: {}", resolved.display()));' + "`r`n" +
"    }" + "`r`n" +
"    Ok(resolved.to_string_lossy().to_string())" + "`r`n" +
"}"

if ($content.Contains($old)) {
    $content = $content.Replace($old, $new)
    Set-Content $file $content -NoNewline
    Write-Host "OK"
} else {
    Write-Host "NOT FOUND"
}
