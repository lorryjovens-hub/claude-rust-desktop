$env:ANDROID_SDK_ROOT = "C:\Android\Sdk"
$env:ANDROID_NDK_ROOT = "C:\Android\Sdk\ndk\27.1.12297006"
$env:PATH = "C:\Android\Sdk\ndk\27.1.12297006\toolchains\llvm\prebuilt\windows-x86_64\bin;" + $env:PATH

# Set the correct linker and ar
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "aarch64-linux-android24-clang.cmd"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_AR = "llvm-ar"

Set-Location "D:\user\Documents\claude-desktop-app-main (2)\claude-rust-desktop-local\src-tauri"

# Build the Rust library for Android
cargo build --target aarch64-linux-android --release --lib

# Check if build succeeded
if ($LASTEXITCODE -eq 0) {
    Write-Host "Rust build succeeded!"
    
    # Copy the built library to the Android project
    $src = "target\aarch64-linux-android\release\libclaude_desktop_tauri_lib.so"
    $dst = "gen\android\app\src\main\jniLibs\arm64-v8a\libclaude_desktop_tauri_lib.so"
    
    New-Item -ItemType Directory -Force -Path (Split-Path $dst)
    Copy-Item $src $dst -Force
    
    Write-Host "Library copied to Android project"
} else {
    Write-Host "Rust build failed!"
}
