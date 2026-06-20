use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::{AUDIO_EXTENSIONS, TARGET_SIZE_BYTES};

const FALLBACK_SIZE: u64 = 15 * 1024 * 1024;

/// Audio bitrate ladder (highest quality first).
const AUDIO_BITRATES: &[u32] = &[320, 256, 192, 160, 128, 96];

pub struct Compressor {
    input_path: PathBuf,
    output_path: PathBuf,
    is_audio: bool,
    cancel: Arc<AtomicBool>,
}

impl Compressor {
    pub fn new(input_path: PathBuf) -> Self {
        let is_audio = input_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false);

        let output_path = Self::output_path(&input_path, is_audio);

        Self {
            input_path,
            output_path,
            is_audio,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn output_path(input: &Path, is_audio: bool) -> PathBuf {
        let parent = input.parent().unwrap_or(Path::new("."));
        let stem = input.file_stem().unwrap_or_default().to_string_lossy();

        if is_audio {
            parent.join(format!("{stem}_10mb.mp3"))
        } else {
            let ext = input.extension().unwrap_or_default().to_string_lossy();
            parent.join(format!("{stem}_10mb.{ext}"))
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn run<F, G>(&self, progress: F, done: G)
    where
        F: Fn(String) + Send + 'static,
        G: FnOnce(Result<String, String>) + Send + 'static,
    {
        let input = self.input_path.clone();
        let output = self.output_path.clone();
        let is_audio = self.is_audio;
        let cancel = self.cancel.clone();

        thread::spawn(move || {
            let result = Self::compress(&input, &output, is_audio, &cancel, &progress);
            done(result);
        });
    }

    fn compress<F>(
        input: &Path,
        output: &Path,
        is_audio: bool,
        cancel: &Arc<AtomicBool>,
        progress: &F,
    ) -> Result<String, String>
    where
        F: Fn(String),
    {
        let original_size = std::fs::metadata(input)
            .map(|m| m.len())
            .map_err(|e| format!("Cannot read file: {e}"))?;

        if original_size <= TARGET_SIZE_BYTES {
            return Err(format!(
                "File is already under 10MB ({}KB)",
                original_size / 1024
            ));
        }

        // Probe duration with ffprobe for smart bitrate estimation
        progress("Analyzing file...".into());
        let duration_secs = probe_duration(input).unwrap_or(0.0);

        if is_audio {
            Self::compress_audio(input, output, cancel, progress, duration_secs)
        } else {
            Self::compress_video(
                input,
                output,
                cancel,
                progress,
                duration_secs,
                original_size,
            )
        }
    }

    /// Audio: estimate required bitrate from duration, jump straight to the right level.
    fn compress_audio<F>(
        input: &Path,
        output: &Path,
        cancel: &Arc<AtomicBool>,
        progress: &F,
        duration_secs: f64,
    ) -> Result<String, String>
    where
        F: Fn(String),
    {
        let input_str = input.to_string_lossy().to_string();
        let output_str = output.to_string_lossy().to_string();

        // Estimate target bitrate: bitrate = (target_bytes * 8) / duration_secs / 1000 (kbps)
        // Add 10% overhead for container/headers
        let target_kbps = if duration_secs > 0.0 {
            ((TARGET_SIZE_BYTES as f64 * 8.0) / duration_secs / 1000.0 * 0.90) as u32
        } else {
            320 // fallback: start high
        };

        // Pick the highest bitrate that's at or below the estimate
        let start_idx = AUDIO_BITRATES
            .iter()
            .position(|&b| b <= target_kbps)
            .unwrap_or(AUDIO_BITRATES.len() - 1);

        for (i, &bitrate) in AUDIO_BITRATES[start_idx..].iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                return Err("Cancelled".into());
            }

            let total = AUDIO_BITRATES.len() - start_idx;
            progress(format!("Compressing {bitrate}kbps ({}/{total})...", i + 1));

            let _ = std::fs::remove_file(output);

            let mut cmd = Command::new("ffmpeg");
            cmd.args([
                "-y",
                "-i",
                &input_str,
                "-b:a",
                &format!("{bitrate}k"),
                "-codec:a",
                "libmp3lame",
                "-threads",
                "0",
                &output_str,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
            #[cfg(windows)]
            {
                cmd.creation_flags(0x08000000);
            }

            if cmd.status().map(|s| s.success()).unwrap_or(false) {
                if let Ok(meta) = std::fs::metadata(output) {
                    let size = meta.len();
                    if size <= TARGET_SIZE_BYTES {
                        return Ok(output.to_string_lossy().to_string());
                    }
                    if size < FALLBACK_SIZE && std::fs::metadata(output).is_ok() {
                        return Ok(output.to_string_lossy().to_string());
                    }
                }
            }
        }

        Err("Could not compress audio below 10MB".into())
    }

    /// Video: estimate compression ratio, jump to appropriate escalation level.
    fn compress_video<F>(
        input: &Path,
        output: &Path,
        cancel: &Arc<AtomicBool>,
        progress: &F,
        _duration_secs: f64,
        original_size: u64,
    ) -> Result<String, String>
    where
        F: Fn(String),
    {
        let input_str = input.to_string_lossy().to_string();
        let output_str = output.to_string_lossy().to_string();

        let ratio = original_size as f64 / TARGET_SIZE_BYTES as f64;

        // Jump to the right escalation level based on how aggressive we need to be
        let start_level: usize = if ratio < 2.0 {
            0 // gentle CRF 23
        } else if ratio < 4.0 {
            2 // scale 854p, CRF 24
        } else if ratio < 8.0 {
            4 // scale 640p, CRF 28
        } else if ratio < 16.0 {
            6 // scale 480p, CRF 30, AAC 96k
        } else {
            7 // scale 480p, ultrafast, CRF 32
        };

        let commands = video_commands(&input_str, &output_str);
        let total = commands.len();

        for i in start_level..commands.len() {
            if cancel.load(Ordering::SeqCst) {
                return Err("Cancelled".into());
            }

            progress(format!("Compressing ({}/{total})...", i + 1));

            let _ = std::fs::remove_file(output);

            let mut cmd = Command::new("ffmpeg");
            cmd.args(&commands[i])
                .arg("-threads")
                .arg("0")
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                cmd.creation_flags(0x08000000);
            }

            if cmd.status().map(|s| s.success()).unwrap_or(false) {
                if let Ok(meta) = std::fs::metadata(output) {
                    let size = meta.len();
                    if size <= TARGET_SIZE_BYTES {
                        return Ok(output.to_string_lossy().to_string());
                    }
                    if size < FALLBACK_SIZE && std::fs::metadata(output).is_ok() {
                        return Ok(output.to_string_lossy().to_string());
                    }
                }
            }
        }

        Err("Could not compress video below 10MB".into())
    }
}

/// Get media duration in seconds using ffprobe.
fn probe_duration(path: &Path) -> Option<f64> {
    let path_str = path.to_string_lossy();
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v",
        "quiet",
        "-show_entries",
        "format=duration",
        "-of",
        "csv=p=0",
        &path_str,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }

    let output = cmd.output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<f64>().ok()
}

fn video_commands(input: &str, output: &str) -> Vec<Vec<String>> {
    macro_rules! v { ($($x:expr),* $(,)?) => { vec![$($x.to_string()),*] }; }

    vec![
        // 0: CRF 23, copy audio
        v![
            "-y",
            "-i",
            input,
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "23",
            "-c:a",
            "copy",
            output
        ],
        // 1: Scale 1280p
        v![
            "-y",
            "-i",
            input,
            "-vf",
            "scale=1280:-2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "23",
            "-c:a",
            "copy",
            output
        ],
        // 2: Scale 854p, CRF 24
        v![
            "-y",
            "-i",
            input,
            "-vf",
            "scale=854:-2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "24",
            "-c:a",
            "copy",
            output
        ],
        // 3: Scale 640p, CRF 26
        v![
            "-y",
            "-i",
            input,
            "-vf",
            "scale=640:-2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "26",
            "-c:a",
            "copy",
            output
        ],
        // 4: Scale 640p, CRF 28
        v![
            "-y",
            "-i",
            input,
            "-vf",
            "scale=640:-2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "28",
            "-c:a",
            "copy",
            output
        ],
        // 5: Scale 480p, CRF 30, AAC 96k
        v![
            "-y",
            "-i",
            input,
            "-vf",
            "scale=480:-2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "30",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            output
        ],
        // 6: Scale 480p, CRF 32, AAC 64k
        v![
            "-y",
            "-i",
            input,
            "-vf",
            "scale=480:-2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "32",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            output
        ],
        // 7: Scale 320p, CRF 35, AAC 48k
        v![
            "-y",
            "-i",
            input,
            "-vf",
            "scale=320:-2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "35",
            "-c:a",
            "aac",
            "-b:a",
            "48k",
            output
        ],
    ]
}
