use anyhow::{anyhow, Result};
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};

use crate::ActionRequest;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct DesktopEngine;

impl DesktopEngine {
    pub fn new() -> Self {
        Self
    }

    fn create_enigo(&self) -> Result<Enigo> {
        Enigo::new(&Settings::default())
            .map_err(|e| anyhow!("Failed to create Enigo: {:?}", e))
    }

    pub async fn execute(&self, req: &ActionRequest) -> Result<()> {
        match req.action_type.as_str() {
            "mouse_move" => self.mouse_move(req).await,
            "mouse_click" => self.mouse_click(req).await,
            "mouse_down" => self.mouse_down(req).await,
            "mouse_up" => self.mouse_up(req).await,
            "mouse_scroll" => self.mouse_scroll(req).await,
            "key_press" => self.key_press(req).await,
            "key_down" => self.key_down(req).await,
            "key_up" => self.key_up(req).await,
            "type_text" => self.type_text(req).await,
            "screenshot" => Ok(()),
            "wait" => {
                let ms = req.duration_ms.unwrap_or(1000);
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                Ok(())
            }
            _ => Err(anyhow!("Unknown action type: {}", req.action_type)),
        }
    }

    async fn mouse_move(&self, req: &ActionRequest) -> Result<()> {
        let coord = req
            .coordinate
            .ok_or_else(|| anyhow!("mouse_move requires coordinate"))?;
        let mut enigo = self.create_enigo()?;
        enigo
            .move_mouse(coord[0], coord[1], Coordinate::Abs)
            .map_err(|e| anyhow!("Failed to move mouse: {:?}", e))?;
        Ok(())
    }

    async fn mouse_click(&self, req: &ActionRequest) -> Result<()> {
        if let Some(coord) = &req.coordinate {
            let mut enigo = self.create_enigo()?;
            enigo
                .move_mouse(coord[0], coord[1], Coordinate::Abs)
                .map_err(|e| anyhow!("Failed to move mouse for click: {:?}", e))?;
        }
        let button = map_button(req.button.as_deref());
        let mut enigo = self.create_enigo()?;
        enigo
            .button(button, Direction::Click)
            .map_err(|e| anyhow!("Failed to click: {:?}", e))?;
        Ok(())
    }

    async fn mouse_down(&self, req: &ActionRequest) -> Result<()> {
        let button = map_button(req.button.as_deref());
        let mut enigo = self.create_enigo()?;
        enigo
            .button(button, Direction::Press)
            .map_err(|e| anyhow!("Failed to press button: {:?}", e))?;
        Ok(())
    }

    async fn mouse_up(&self, req: &ActionRequest) -> Result<()> {
        let button = map_button(req.button.as_deref());
        let mut enigo = self.create_enigo()?;
        enigo
            .button(button, Direction::Release)
            .map_err(|e| anyhow!("Failed to release button: {:?}", e))?;
        Ok(())
    }

    async fn mouse_scroll(&self, req: &ActionRequest) -> Result<()> {
        if let Some(scroll_y) = req.scroll_y {
            let mut enigo = self.create_enigo()?;
            enigo
                .scroll(scroll_y, enigo::Axis::Vertical)
                .map_err(|e| anyhow!("Failed to scroll vertical: {:?}", e))?;
        }
        if let Some(scroll_x) = req.scroll_x {
            let mut enigo = self.create_enigo()?;
            enigo
                .scroll(scroll_x, enigo::Axis::Horizontal)
                .map_err(|e| anyhow!("Failed to scroll horizontal: {:?}", e))?;
        }
        Ok(())
    }

    async fn key_press(&self, req: &ActionRequest) -> Result<()> {
        let key = req
            .key
            .as_deref()
            .ok_or_else(|| anyhow!("key_press requires key"))?;
        let mut enigo = self.create_enigo()?;
        if key.len() == 1 {
            enigo
                .text(key)
                .map_err(|e| anyhow!("Failed to type character: {:?}", e))?;
        } else {
            let enigo_key = parse_key(key)?;
            enigo
                .key(enigo_key, Direction::Click)
                .map_err(|e| anyhow!("Failed to press key: {:?}", e))?;
        }
        Ok(())
    }

    async fn key_down(&self, req: &ActionRequest) -> Result<()> {
        let key = req
            .key
            .as_deref()
            .ok_or_else(|| anyhow!("key_down requires key"))?;
        let enigo_key = parse_key(key)?;
        let mut enigo = self.create_enigo()?;
        enigo
            .key(enigo_key, Direction::Press)
            .map_err(|e| anyhow!("Failed to press key down: {:?}", e))?;
        Ok(())
    }

    async fn key_up(&self, req: &ActionRequest) -> Result<()> {
        let key = req
            .key
            .as_deref()
            .ok_or_else(|| anyhow!("key_up requires key"))?;
        let enigo_key = parse_key(key)?;
        let mut enigo = self.create_enigo()?;
        enigo
            .key(enigo_key, Direction::Release)
            .map_err(|e| anyhow!("Failed to release key: {:?}", e))?;
        Ok(())
    }

    async fn type_text(&self, req: &ActionRequest) -> Result<()> {
        let text = req
            .text
            .as_deref()
            .ok_or_else(|| anyhow!("type_text requires text"))?;
        let mut enigo = self.create_enigo()?;
        enigo
            .text(text)
            .map_err(|e| anyhow!("Failed to type text: {:?}", e))?;
        Ok(())
    }

    pub async fn take_screenshot(&self) -> Result<String> {
        take_screenshot_powershell()
    }
}

fn map_button(button: Option<&str>) -> Button {
    match button.unwrap_or("left") {
        "left" => Button::Left,
        "right" => Button::Right,
        "middle" => Button::Middle,
        "back" => Button::Back,
        "forward" => Button::Forward,
        _ => Button::Left,
    }
}

fn parse_key(key: &str) -> Result<Key> {
    match key.to_lowercase().as_str() {
        "enter" | "return" => Ok(Key::Return),
        "tab" => Ok(Key::Tab),
        "escape" | "esc" => Ok(Key::Escape),
        "backspace" => Ok(Key::Backspace),
        "delete" | "del" => Ok(Key::Delete),
        "home" => Ok(Key::Home),
        "end" => Ok(Key::End),
        "pageup" | "page_up" => Ok(Key::PageUp),
        "pagedown" | "page_down" => Ok(Key::PageDown),
        "up" | "arrowup" => Ok(Key::UpArrow),
        "down" | "arrowdown" => Ok(Key::DownArrow),
        "left" | "arrowleft" => Ok(Key::LeftArrow),
        "right" | "arrowright" => Ok(Key::RightArrow),
        "f1" => Ok(Key::F1),
        "f2" => Ok(Key::F2),
        "f3" => Ok(Key::F3),
        "f4" => Ok(Key::F4),
        "f5" => Ok(Key::F5),
        "f6" => Ok(Key::F6),
        "f7" => Ok(Key::F7),
        "f8" => Ok(Key::F8),
        "f9" => Ok(Key::F9),
        "f10" => Ok(Key::F10),
        "f11" => Ok(Key::F11),
        "f12" => Ok(Key::F12),
        "shift" => Ok(Key::Shift),
        "control" | "ctrl" => Ok(Key::Control),
        "alt" => Ok(Key::Alt),
        "super" | "meta" | "win" => Ok(Key::Meta),
        "capslock" | "caps_lock" => Ok(Key::CapsLock),
        "space" => Ok(Key::Space),
        "insert" => Ok(Key::Insert),
        "printscreen" | "print_screen" | "snapshot" => Ok(Key::Snapshot),
        "scrolllock" | "scroll_lock" => Ok(Key::Scroll),
        "numlock" | "num_lock" => Ok(Key::Numlock),
        "pause" | "break" => Ok(Key::Pause),
        _ => Err(anyhow!("Unknown key: {}", key)),
    }
}

pub fn take_screenshot_powershell() -> Result<String> {
    let ps_script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$g.Dispose()
$ms = New-Object System.IO.MemoryStream
$bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Jpeg)
$bmp.Dispose()
[Convert]::ToBase64String($ms.ToArray())
"#;

    let mut cmd = std::process::Command::new("powershell");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
        .map_err(|e| anyhow!("Failed to spawn PowerShell for screenshot: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Screenshot failed: {}", stderr));
    }

    let base64_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if base64_str.is_empty() {
        return Err(anyhow!("Screenshot produced empty output"));
    }

    Ok(base64_str)
}