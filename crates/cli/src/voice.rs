//! Native device adapter. A dedicated thread owns CPAL's non-Send stream.
use anyhow::{bail, ensure, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::oneshot;

pub struct Recording {
    stop: Arc<AtomicBool>,
    result: oneshot::Receiver<Result<Vec<u8>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Recording {
    pub async fn start() -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = oneshot::channel();
        let (result_tx, result) = oneshot::channel();
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut ready = Some(ready_tx);
            let output = (|| -> Result<Vec<u8>> {
                let device = cpal::default_host()
                    .default_input_device()
                    .context("No microphone is available")?;
                let config = device
                    .default_input_config()
                    .context("Microphone permission or configuration is unavailable")?;
                let rate = config.sample_rate().0;
                let channels = config.channels();
                let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
                let error = Arc::new(Mutex::new(None::<String>));
                let failure = error.clone();
                let on_error = move |e: cpal::StreamError| {
                    *failure.lock().unwrap() = Some(e.to_string());
                };
                let max = claude_core::services::voice::MAX_AUDIO_BYTES / 2 - 22;
                macro_rules! stream {
                    ($t:ty,$convert:expr) => {{
                        let samples = buffer.clone();
                        let stop = stopped.clone();
                        device.build_input_stream(
                            &config.clone().into(),
                            move |data: &[$t], _| {
                                let mut samples = samples.lock().unwrap();
                                for &sample in data.iter().take(max.saturating_sub(samples.len())) {
                                    samples.push(($convert)(sample));
                                }
                                if samples.len() >= max {
                                    stop.store(true, Ordering::SeqCst);
                                }
                            },
                            on_error,
                            None,
                        )?
                    }};
                }
                let stream = match config.sample_format() {
                    cpal::SampleFormat::F32 => {
                        stream!(f32, |v: f32| (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                    }
                    cpal::SampleFormat::I16 => stream!(i16, |v: i16| v),
                    cpal::SampleFormat::U16 => stream!(u16, |v: u16| (v as i32 - 32768) as i16),
                    other => bail!("Unsupported microphone sample format: {other:?}"),
                };
                stream.play()?;
                let _ = ready.take().unwrap().send(Ok(()));
                while !stopped.load(Ordering::SeqCst) {
                    if error.lock().unwrap().is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                drop(stream);
                if let Some(error) = error.lock().unwrap().take() {
                    bail!("Microphone failed: {error}");
                }
                let samples = buffer.lock().unwrap();
                ensure!(!samples.is_empty(), "Microphone returned no audio");
                Ok(wav(&samples, rate, channels))
            })();
            if let Some(ready) = ready {
                let _ = ready.send(Err(anyhow::anyhow!(
                    "{}",
                    output
                        .as_ref()
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_default()
                )));
            }
            let _ = result_tx.send(output);
        });
        let recording = Self {
            stop,
            result,
            thread: Some(thread),
        };
        ready_rx.await.context("Microphone thread exited")??;
        Ok(recording)
    }
    pub async fn finish(mut self) -> Result<Vec<u8>> {
        self.stop.store(true, Ordering::SeqCst);
        (&mut self.result)
            .await
            .context("Microphone thread exited")?
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn wav(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
    let size = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(size as usize + 44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(size + 36).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&size.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

pub async fn run(
    cli: &crate::args::Cli,
    paths: &claude_core::runtime::RuntimePaths,
    context: &claude_core::services::ServiceContext,
    file: &Option<std::path::PathBuf>,
    push_to_talk: bool,
    continuous: bool,
    send: bool,
) -> Result<()> {
    use std::io::IsTerminal;
    ensure!(
        !continuous || file.is_none(),
        "Continuous mode records the microphone; do not pass --file"
    );
    ensure!(
        file.is_some() || std::io::stdin().is_terminal(),
        "Microphone mode requires interactive stdin; use --file for scripts"
    );
    let config: claude_core::services::voice::VoiceConfig = context
        .setting("voice")?
        .context("Voice backend is not configured; use voice-config")?;
    config.validate()?;
    context.credentials.get(&config.credential)?;
    let resumed = if send {
        if let Some(id) = &cli.resume {
            let db = paths.open_database()?;
            let resumed = claude_core::runtime::SessionStore::new(db.clone()).resume(id)?;
            let (workspace, _) = db.with_conn(|c| {
                claude_core::native_engine::workspace::resolve_workspace(
                    c,
                    &paths.data_dir.join("workspaces"),
                    id,
                    None,
                )
            })??;
            if cli.path.is_some() {
                ensure!(
                    std::fs::canonicalize(&workspace)?
                        == std::fs::canonicalize(std::env::current_dir()?)?,
                    "--path differs from the resumed session's project"
                );
            }
            std::env::set_current_dir(workspace)?;
            Some(resumed)
        } else {
            None
        }
    } else {
        None
    };
    let session = if send {
        Some(
            crate::session::Session::open(
                cli,
                paths,
                crate::config::select(
                    cli,
                    paths,
                    resumed.as_ref().and_then(|s| s.model.as_deref()),
                    resumed.as_ref().and_then(|s| s.provider.as_deref()),
                )?,
            )
            .await?,
        )
    } else {
        None
    };
    let mut input = if file.is_none() {
        Some(crate::session::terminal_input())
    } else {
        None
    };
    let operation = async {
        loop {
            let (bytes, mime) = if let Some(file) = file {
                let size = std::fs::metadata(file)?.len();
                ensure!(
                    size <= claude_core::services::voice::MAX_AUDIO_BYTES as u64,
                    "Audio exceeds 25 MiB"
                );
                let mime = match file.extension().and_then(|s| s.to_str()) {
                    Some("wav") => "audio/wav",
                    Some("webm") => "audio/webm",
                    Some("ogg") => "audio/ogg",
                    Some("mp3") => "audio/mpeg",
                    Some("mp4" | "m4a") => "audio/mp4",
                    _ => bail!("Unsupported audio file extension"),
                };
                (std::fs::read(file)?, mime)
            } else {
                if push_to_talk || continuous {
                    eprintln!("Press Enter to record, /exit to quit.");
                    if input
                        .as_mut()
                        .unwrap()
                        .recv()
                        .await
                        .as_deref()
                        .is_none_or(|s| s.trim() == "/exit")
                    {
                        break;
                    }
                }
                let recording = Recording::start().await?;
                eprintln!("Recording; press Enter to transcribe, Ctrl+C to cancel.");
                let line = input.as_mut().unwrap().recv().await;
                let bytes = recording.finish().await?;
                if line.is_none() {
                    break;
                }
                (bytes, "audio/wav")
            };
            eprintln!("Transcribing…");
            let text = claude_core::services::voice::transcribe(context, bytes, mime).await?;
            if let Some(session) = &session {
                session.turn(cli, &text, &mut input).await?;
            } else {
                crate::services::print(&serde_json::json!({"text":text}))?;
            }
            if !continuous {
                break;
            }
        }
        Ok::<_, anyhow::Error>(())
    };
    let result = tokio::select! {result=operation=>result,_=tokio::signal::ctrl_c()=>Ok(())};
    if let Some(session) = session {
        session.close().await?;
    }
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wav_preserves_pcm_sample_rate_channels_and_data() {
        let data = wav(&[-32768, 0, 32767], 16000, 1);
        assert_eq!(&data[..4], b"RIFF");
        assert_eq!(&data[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 16000);
        assert_eq!(u32::from_le_bytes(data[40..44].try_into().unwrap()), 6);
        assert_eq!(&data[44..], &[0, 128, 0, 0, 255, 127]);
    }
    #[tokio::test]
    #[ignore = "requires a real default microphone; captures 250 ms locally without uploading"]
    async fn native_microphone_capture_releases_the_device() {
        let recording = Recording::start()
            .await
            .expect("default microphone is unavailable");
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let audio = recording.finish().await.unwrap();
        assert!(audio.len() > 44);
        assert_eq!(&audio[..4], b"RIFF");
        let second = Recording::start().await.expect("device was not released");
        drop(second);
    }
}
