$env:ANDROID_SDK_ROOT = "C:\Users\user\AppData\Local\Android\Sdk"
$env:ANDROID_NDK_ROOT = "C:\Users\user\AppData\Local\Android\Sdk\ndk\29.0.13113456"
$env:PATH = "C:\Users\user\AppData\Local\Android\Sdk\ndk\29.0.13113456\toolchains\llvm\prebuilt\windows-x86_64\bin;" + $env:PATH
$env:RING_PREGENERATE_ASM = "1"
$env:CC = "D:\user\Documents\claude-desktop-app-main (2)\claude-rust-desktop-local\src-tauri\.cargo\clang_wrapper.cmd"
$env:CXX = "D:\user\Documents\claude-desktop-app-main (2)\claude-rust-desktop-local\src-tauri\.cargo\clang_wrapper.cmd"
Set-Location "D:\user\Documents\claude-desktop-app-main (2)\claude-rust-desktop-local\src-tauri"
Remove-Item -Recurse -Force "target" -ErrorAction SilentlyContinue
Remove-Item -Force "Cargo.lock" -ErrorAction SilentlyContinue
cargo build --target aarch64-linux-android --release
