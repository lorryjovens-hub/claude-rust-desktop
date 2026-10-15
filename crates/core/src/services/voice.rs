use super::{validate_endpoint, ServiceContext};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VoiceConfig {
    /// Full OpenAI-compatible audio transcription URL, e.g. /v1/audio/transcriptions.
    pub endpoint: String,
    pub model: String,
    pub credential: String,
    pub language: Option<String>,
}
impl VoiceConfig {
    pub fn validate(&self) -> Result<()> {
        validate_endpoint(&self.endpoint)?;
        ensure!(
            !self.model.trim().is_empty() && !self.credential.trim().is_empty(),
            "Model and credential name are required"
        );
        Ok(())
    }
}

pub async fn transcribe(context: &ServiceContext, bytes: Vec<u8>, mime: &str) -> Result<String> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_AUDIO_BYTES,
        "Audio must contain 1..25 MiB"
    );
    let extension = match mime {
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/webm" => "webm",
        "audio/ogg" => "ogg",
        "audio/mp4" => "mp4",
        "audio/mpeg" => "mp3",
        _ => anyhow::bail!("Unsupported audio format: {mime}"),
    };
    let config: VoiceConfig = context
        .setting("voice")?
        .context("Voice backend is not configured; configure an audio transcription endpoint")?;
    config.validate()?;
    let token = context.credentials.get(&config.credential)?;
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(format!("recording.{extension}"))
        .mime_str(mime)?;
    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", config.model)
        .text("response_format", "json");
    if let Some(language) = config.language.filter(|s| !s.is_empty()) {
        form = form.text("language", language);
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .post(config.endpoint)
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await
        .context("Audio transcription connection failed")?;
    ensure!(
        response.status().is_success(),
        "Audio transcription returned HTTP {}",
        response.status()
    );
    #[derive(Deserialize)]
    struct Transcript {
        text: String,
    }
    let text = response
        .json::<Transcript>()
        .await
        .context("Invalid audio transcription response")?
        .text;
    ensure!(!text.trim().is_empty(), "No speech was recognized");
    Ok(text)
}
