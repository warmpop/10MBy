#![windows_subsystem = "windows"]

mod clipboard;
mod compressor;
mod gui;
mod registry;

use clap::Parser;
use std::path::PathBuf;

const TARGET_SIZE_BYTES: u64 = (9.99 * 1024.0 * 1024.0) as u64;

pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma"];
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "m4v"];

#[derive(Parser, Debug)]
#[command(name = "10MBy", about = "Compress video/audio to fit under 10MB")]
struct Args {
    /// Path to a media file to compress
    file: Option<String>,

    /// Install context menu entries
    #[arg(long)]
    install: bool,

    /// Uninstall context menu entries
    #[arg(long)]
    uninstall: bool,
}

fn main() {
    let args = Args::parse();

    if args.install {
        registry::install().unwrap_or_else(|e| eprintln!("Install failed: {e}"));
        return;
    }

    if args.uninstall {
        registry::uninstall().unwrap_or_else(|e| eprintln!("Uninstall failed: {e}"));
        return;
    }

    // Auto-install context menu on first launch (when no file is passed)
    if args.file.is_none() && !registry::is_installed() {
        let _ = registry::install();
    }

    // Check if ffmpeg is available
    let ffmpeg_ok = std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    // Determine input file
    let input_file = args.file.map(PathBuf::from).filter(|p| p.is_file());

    // Check for auto-close: non-interactive mode when a file is passed
    let auto_close = input_file.is_some();

    // Run the GUI
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 240.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_title("10MBy")
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "10MBy",
        native_options,
        Box::new(move |_cc| Ok(Box::new(gui::App::new(input_file, auto_close, ffmpeg_ok)))),
    )
    .expect("Failed to launch GUI");
}

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../10mby.png");
    image_from_png(bytes).unwrap_or_default()
}

fn image_from_png(bytes: &[u8]) -> Result<egui::IconData, ()> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|_| ())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|_| ())?;
    Ok(egui::IconData {
        rgba: buf[..info.buffer_size()].to_vec(),
        width: info.width,
        height: info.height,
    })
}
