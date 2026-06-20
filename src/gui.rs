use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use egui::{Color32, Frame, RichText, Rounding, Vec2};

use crate::clipboard;
use crate::compressor::Compressor;

enum UiState {
    Welcome {
        ffmpeg_ok: bool,
    },
    Compressing {
        filename: String,
        status: String,
        compressor: Arc<Compressor>,
        progress_rx: Receiver<String>,
    },
    Done {
        success: bool,
        message: String,
        output_path: Option<String>,
    },
}

pub struct App {
    state: UiState,
    auto_close: bool,
    request_open_file: bool,
    settings_open: bool,
}

impl App {
    pub fn new(input_file: Option<PathBuf>, auto_close: bool, ffmpeg_ok: bool) -> Self {
        if let Some(path) = input_file {
            let state = Self::begin_compression(path);
            Self {
                state,
                auto_close,
                request_open_file: false,
                settings_open: false,
            }
        } else {
            Self {
                state: UiState::Welcome { ffmpeg_ok },
                auto_close,
                request_open_file: false,
                settings_open: false,
            }
        }
    }

    fn begin_compression(path: PathBuf) -> UiState {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let compressor = Arc::new(Compressor::new(path));
        let (tx, rx) = mpsc::channel();

        let tx_done = tx.clone();

        compressor.run(
            move |status| {
                let _ = tx.send(status);
            },
            move |result| {
                let msg = match result {
                    Ok(p) => format!("OK:{p}"),
                    Err(e) => format!("ERR:{e}"),
                };
                let _ = tx_done.send(msg);
            },
        );

        UiState::Compressing {
            filename,
            status: "Starting...".into(),
            compressor,
            progress_rx: rx,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.request_open_file {
            self.request_open_file = false;
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Media files",
                    &[
                        "mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "m4v", "mp3", "wav",
                        "flac", "aac", "ogg", "m4a", "wma",
                    ],
                )
                .add_filter("All files", &["*"])
                .pick_file()
            {
                self.state = Self::begin_compression(path);
            }
        }

        if matches!(&self.state, UiState::Welcome { .. }) && !self.settings_open {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            for file in dropped {
                if let Some(path) = file.path {
                    if path.is_file() {
                        self.state = Self::begin_compression(path);
                        break;
                    }
                }
            }
        }

        if let UiState::Compressing {
            progress_rx,
            status,
            ..
        } = &mut self.state
        {
            while let Ok(msg) = progress_rx.try_recv() {
                if let Some(rest) = msg.strip_prefix("OK:") {
                    let path = rest.to_string();
                    clipboard::copy_file_to_clipboard(&PathBuf::from(&path));
                    self.state = UiState::Done {
                        success: true,
                        message: path.clone(),
                        output_path: Some(path),
                    };
                    break;
                } else if let Some(rest) = msg.strip_prefix("ERR:") {
                    self.state = UiState::Done {
                        success: false,
                        message: rest.to_string(),
                        output_path: None,
                    };
                    break;
                } else {
                    *status = msg;
                }
            }
            ctx.request_repaint();
        }

        if self.auto_close {
            if let UiState::Done { success, .. } = &self.state {
                if *success {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            }
        }

        ctx.set_visuals(egui::Visuals::dark());

        let mut cancel_requested = false;

        egui::CentralPanel::default()
            .frame(Frame::none().inner_margin(20.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| match &self.state {
                    UiState::Welcome { ffmpeg_ok } => {
                        draw_welcome(
                            ui,
                            *ffmpeg_ok,
                            &mut self.request_open_file,
                            &mut self.settings_open,
                        );
                    }
                    UiState::Compressing {
                        filename, status, ..
                    } => {
                        cancel_requested = draw_progress(ui, filename, status);
                    }
                    UiState::Done {
                        success,
                        message,
                        output_path,
                    } => {
                        draw_done(ui, *success, message, output_path.as_deref());
                    }
                });
            });

        if cancel_requested {
            if let UiState::Compressing { compressor, .. } = &self.state {
                compressor.cancel();
            }
            self.state = UiState::Done {
                success: false,
                message: "Cancelled".into(),
                output_path: None,
            };
        }
    }
}

// ── Draw functions ──

fn draw_welcome(
    ui: &mut egui::Ui,
    ffmpeg_ok: bool,
    request_open: &mut bool,
    settings_open: &mut bool,
) {
    if *settings_open {
        draw_settings(ui, settings_open);
        return;
    }

    // Settings gear button in top-right corner
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        let gear_btn = egui::Button::new(RichText::new("\u{2699}").size(16.0))
            .min_size(Vec2::new(28.0, 28.0))
            .rounding(Rounding::same(4.0))
            .fill(Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE);
        if ui.add(gear_btn).clicked() {
            *settings_open = true;
        }
    });

    ui.add_space(16.0);

    if ffmpeg_ok {
        ui.label(
            RichText::new("10MBy")
                .size(28.0)
                .strong()
                .color(Color32::from_rgb(0xe0, 0xe0, 0xe0)),
        );
    } else {
        ui.label(
            RichText::new("FFmpeg not found!")
                .size(22.0)
                .strong()
                .color(Color32::from_rgb(0xff, 0x55, 0x55)),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("Install ffmpeg and restart 10MBy")
                .size(12.0)
                .color(Color32::from_rgb(0x88, 0x88, 0x88)),
        );
        return;
    }

    ui.add_space(6.0);
    ui.label(
        RichText::new("Drag a file here or click to browse")
            .size(12.0)
            .color(Color32::from_rgb(0x88, 0x88, 0x88)),
    );

    ui.add_space(16.0);

    let open_btn = egui::Button::new(RichText::new("  Open File  ").size(14.0))
        .min_size(Vec2::new(140.0, 36.0))
        .rounding(Rounding::same(6.0))
        .fill(Color32::from_rgb(0x3a, 0x3a, 0x3a));

    if ui.add(open_btn).clicked() {
        *request_open = true;
    }

    ui.add_space(20.0);
    ui.label(
        RichText::new("Formats: MP4, MKV, AVI, MOV, WEBM, MP3, WAV, FLAC, ...")
            .size(10.0)
            .color(Color32::from_rgb(0x55, 0x55, 0x55)),
    );
}

fn draw_settings(ui: &mut egui::Ui, settings_open: &mut bool) {
    ui.add_space(20.0);

    ui.label(
        RichText::new("Settings")
            .size(20.0)
            .strong()
            .color(Color32::from_rgb(0xe0, 0xe0, 0xe0)),
    );

    ui.add_space(20.0);

    if ui
        .add(
            egui::Button::new(RichText::new("Uninstall 10MBy").size(13.0))
                .min_size(Vec2::new(160.0, 34.0))
                .rounding(Rounding::same(4.0))
                .fill(Color32::from_rgb(0x55, 0x30, 0x30)),
        )
        .clicked()
    {
        // Launch setup.exe (separate process) to handle full uninstall
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let setup = PathBuf::from(local)
                .join("Programs")
                .join("10MBy")
                .join("setup.exe");
            if setup.exists() {
                let _ = std::process::Command::new(&setup).spawn();
            }
        }
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }

    ui.add_space(12.0);

    if ui
        .add(
            egui::Button::new(RichText::new("Back").size(12.0))
                .min_size(Vec2::new(100.0, 28.0))
                .rounding(Rounding::same(4.0))
                .fill(Color32::from_rgb(0x3a, 0x3a, 0x3a)),
        )
        .clicked()
    {
        *settings_open = false;
    }
}

fn draw_progress(ui: &mut egui::Ui, filename: &str, status: &str) -> bool {
    let mut cancel = false;

    ui.add_space(20.0);

    ui.label(
        RichText::new(filename)
            .size(14.0)
            .color(Color32::from_rgb(0xe0, 0xe0, 0xe0)),
    );

    ui.add_space(14.0);

    let spinner = egui::Spinner::new()
        .size(36.0)
        .color(Color32::from_rgb(0x88, 0x88, 0xff));
    ui.add(spinner);

    ui.add_space(14.0);

    ui.label(
        RichText::new(status)
            .size(11.0)
            .color(Color32::from_rgb(0x88, 0x88, 0x88)),
    );

    ui.add_space(20.0);

    if ui
        .add(
            egui::Button::new(RichText::new("Cancel").size(12.0))
                .min_size(Vec2::new(80.0, 28.0))
                .rounding(Rounding::same(4.0))
                .fill(Color32::from_rgb(0x55, 0x30, 0x30)),
        )
        .clicked()
    {
        cancel = true;
    }

    cancel
}

fn draw_done(ui: &mut egui::Ui, success: bool, message: &str, output_path: Option<&str>) {
    ui.add_space(20.0);

    if success {
        ui.label(
            RichText::new("Done!")
                .size(22.0)
                .strong()
                .color(Color32::from_rgb(0x4c, 0xaf, 0x50)),
        );

        ui.add_space(8.0);

        if let Some(path) = output_path {
            let filename = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string());

            ui.label(
                RichText::new(filename)
                    .size(12.0)
                    .color(Color32::from_rgb(0xaa, 0xaa, 0xaa)),
            );
        }

        ui.add_space(4.0);
        ui.label(
            RichText::new("Copied to clipboard")
                .size(11.0)
                .color(Color32::from_rgb(0x66, 0x66, 0x66)),
        );
    } else {
        ui.label(
            RichText::new(message)
                .size(14.0)
                .color(Color32::from_rgb(0xff, 0x55, 0x55)),
        );
    }

    ui.add_space(16.0);

    if success {
        if let Some(path) = output_path {
            if ui
                .add(
                    egui::Button::new(RichText::new("Show File").size(12.0))
                        .min_size(Vec2::new(120.0, 28.0))
                        .rounding(Rounding::same(4.0))
                        .fill(Color32::from_rgb(0x3a, 0x3a, 0x3a)),
                )
                .clicked()
            {
                let _ = std::process::Command::new("explorer")
                    .arg("/select,")
                    .arg(std::path::Path::new(path))
                    .spawn();
            }
            ui.add_space(6.0);
        }
    }

    if ui
        .add(
            egui::Button::new(RichText::new("Close").size(12.0))
                .min_size(Vec2::new(120.0, 28.0))
                .rounding(Rounding::same(4.0))
                .fill(Color32::from_rgb(0x3a, 0x3a, 0x3a)),
        )
        .clicked()
    {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
