#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::process::Command;

use egui::{Color32, Frame, RichText, Rounding, Vec2};

fn install_dir() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(local).join("Programs").join("10MBy")
}

fn main_exe_path() -> PathBuf {
    install_dir().join("10MBy.exe")
}

fn is_installed() -> bool {
    main_exe_path().exists()
}

fn has_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn install_ffmpeg(status_cb: &dyn Fn(&str)) -> Result<(), String> {
    status_cb("Installing FFmpeg Essentials...");
    let s1 = Command::new("winget")
        .args([
            "install",
            "--id=Gyan.FFmpeg.Essentials",
            "-e",
            "--accept-source-agreements",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("winget failed: {e}"))?;

    if !s1.success() {
        return Err("FFmpeg Essentials install failed. Try installing manually.".into());
    }

    status_cb("Installing FFmpeg (full)...");
    let s2 = Command::new("winget")
        .args([
            "install",
            "--id=Gyan.FFmpeg",
            "-e",
            "--accept-source-agreements",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("winget failed: {e}"))?;

    if !s2.success() {
        return Err("FFmpeg install failed. Try installing manually.".into());
    }

    Ok(())
}

fn ensure_ffmpeg(status_cb: &dyn Fn(&str)) -> Result<(), String> {
    if has_ffmpeg() {
        return Ok(());
    }
    status_cb("FFmpeg not found. Installing...");
    install_ffmpeg(status_cb)?;
    if !has_ffmpeg() {
        return Err("FFmpeg installed but not found in PATH. Try restarting.".into());
    }
    Ok(())
}

fn do_install(status_cb: &dyn Fn(&str)) -> Result<String, String> {
    ensure_ffmpeg(status_cb)?;

    status_cb("Extracting files...");
    let dest = install_dir();
    std::fs::create_dir_all(&dest).map_err(|e| format!("Cannot create install folder: {e}"))?;

    // Extract embedded main binary and assets
    std::fs::write(
        dest.join("10MBy.exe"),
        include_bytes!("../target/release/10MBy.exe"),
    )
    .map_err(|e| format!("Cannot write exe: {e}"))?;
    std::fs::write(dest.join("10mby.png"), include_bytes!("../10mby.png"))
        .map_err(|e| format!("Cannot write PNG: {e}"))?;
    std::fs::write(dest.join("10mby.ico"), include_bytes!("../10mby.ico"))
        .map_err(|e| format!("Cannot write ICO: {e}"))?;

    // Also copy ourselves for future uninstalls
    let setup_src = std::env::current_exe().map_err(|e| format!("Cannot get exe path: {e}"))?;
    std::fs::copy(&setup_src, dest.join("setup.exe"))
        .map_err(|e| format!("Cannot copy setup: {e}"))?;

    status_cb("Creating shortcut...");
    create_shortcut(&dest)?;

    status_cb("Registering context menu...");
    let status = Command::new(dest.join("10MBy.exe"))
        .arg("--install")
        .status()
        .map_err(|e| format!("Cannot run installer: {e}"))?;

    if status.success() {
        Ok("10MBy installed successfully!".into())
    } else {
        Err("Install completed, but context menu registration may have failed.".into())
    }
}

fn create_shortcut(install_dir: &PathBuf) -> Result<(), String> {
    let shortcut_path = get_start_menu_dir().join("10MBy.lnk");
    let target = install_dir.join("10MBy.exe");
    let working_dir = install_dir.to_string_lossy().to_string();

    let ps_cmd = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{shortcut}'); \
         $s.TargetPath = '{target}'; \
         $s.WorkingDirectory = '{working_dir}'; \
         $s.IconLocation = '{icon}'; \
         $s.Save()",
        shortcut = shortcut_path.to_string_lossy().replace('\'', "''"),
        target = target.to_string_lossy().replace('\'', "''"),
        working_dir = working_dir.replace('\'', "''"),
        icon = target.to_string_lossy().replace('\'', "''"),
    );

    let _ = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_cmd])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    Ok(())
}

fn get_start_menu_dir() -> PathBuf {
    let programs = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(programs)
        .parent()
        .unwrap_or(std::path::Path::new(""))
        .join("Roaming")
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
}

fn do_uninstall() -> Result<String, String> {
    // Unregister context menu
    if main_exe_path().exists() {
        let _ = Command::new(main_exe_path()).arg("--uninstall").status();
    }

    // Delete shortcut
    let shortcut = get_start_menu_dir().join("10MBy.lnk");
    let _ = std::fs::remove_file(&shortcut);

    // Delete install directory
    let dir = install_dir();
    if dir.exists() {
        if std::fs::remove_dir_all(&dir).is_err() {
            // We're running from inside the install dir — can't delete ourselves.
            // Write a batch file to %TEMP% that cleans up after we exit.
            let batch = std::env::temp_dir().join("10mby_cleanup.bat");
            let dir_str = dir.to_string_lossy().to_string();
            let script = format!(
                "@echo off\r\n\
                 cd /d %TEMP%\r\n\
                 ping 127.0.0.1 -n 8 >nul\r\n\
                 rmdir /s /q \"{dir_str}\" 2>nul\r\n\
                 del \"%~f0\"\r\n"
            );
            let _ = std::fs::write(&batch, &script);
            let mut cmd = Command::new("cmd");
            cmd.args([
                "/c",
                &format!("start \"\" /b cmd /c \"{}\"", batch.to_string_lossy()),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            let _ = cmd.spawn();
        }
    }

    Ok("10MBy uninstalled successfully.".into())
}

// Icon / Logo

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

fn logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let bytes = include_bytes!("../10mby.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("Failed to decode logo PNG");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .expect("Failed to read logo PNG");
    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [info.width as usize, info.height as usize],
        &buf[..info.buffer_size()],
    );
    ctx.load_texture("10mby-logo", color_image, egui::TextureOptions::default())
}

// GUI

struct SetupApp {
    status: String,
    error: bool,
    finished: bool,
    was_install: bool,
    logo_loaded: bool,
}

impl SetupApp {
    fn new() -> Self {
        Self {
            status: if is_installed() {
                "10MBy is already installed.".into()
            } else {
                "10MBy is not installed.".into()
            },
            error: false,
            finished: false,
            was_install: false,
            logo_loaded: false,
        }
    }
}

impl eframe::App for SetupApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        if !self.logo_loaded {
            let _ = logo_texture(ctx);
            self.logo_loaded = true;
        }

        egui::CentralPanel::default()
            .frame(Frame::none().inner_margin(24.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);

                    let logo = logo_texture(ctx);
                    ui.add(
                        egui::Image::new(egui::ImageSource::Texture(
                            egui::load::SizedTexture::from_handle(&logo),
                        ))
                        .max_size(Vec2::new(48.0, 48.0)),
                    );

                    ui.add_space(8.0);

                    ui.label(
                        RichText::new("10MBy Installer")
                            .size(22.0)
                            .strong()
                            .color(Color32::from_rgb(0xe0, 0xe0, 0xe0)),
                    );

                    ui.add_space(12.0);

                    let color = if self.error {
                        Color32::from_rgb(0xff, 0x55, 0x55)
                    } else if self.finished {
                        Color32::from_rgb(0x4c, 0xaf, 0x50)
                    } else {
                        Color32::from_rgb(0xaa, 0xaa, 0xaa)
                    };

                    ui.label(RichText::new(&self.status).size(13.0).color(color));

                    ui.add_space(16.0);

                    if !self.finished {
                        let installed = is_installed();

                        let (btn_text, btn_fill) = if installed {
                            ("Uninstall 10MBy", Color32::from_rgb(0x55, 0x30, 0x30))
                        } else {
                            ("Install 10MBy", Color32::from_rgb(0x2e, 0x7d, 0x32))
                        };

                        if ui
                            .add(
                                egui::Button::new(RichText::new(btn_text).size(14.0))
                                    .min_size(Vec2::new(180.0, 36.0))
                                    .rounding(Rounding::same(6.0))
                                    .fill(btn_fill),
                            )
                            .clicked()
                        {
                            let result = if installed {
                                self.was_install = false;
                                do_uninstall()
                            } else {
                                self.was_install = true;
                                do_install(&|_| {})
                            };
                            match result {
                                Ok(msg) => {
                                    self.status = msg;
                                    self.error = false;
                                    self.finished = true;
                                }
                                Err(e) => {
                                    self.status = e;
                                    self.error = true;
                                }
                            }
                        }
                    }

                    if self.finished {
                        ui.add_space(12.0);

                        if self.was_install && !self.error {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("Launch").size(12.0))
                                        .min_size(Vec2::new(120.0, 28.0))
                                        .rounding(Rounding::same(4.0))
                                        .fill(Color32::from_rgb(0x2e, 0x7d, 0x32)),
                                )
                                .clicked()
                            {
                                let _ = Command::new(main_exe_path()).spawn();
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            ui.add_space(6.0);
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
                });
            });
    }
}

fn main() {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([380.0, 280.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_title("10MBy Installer")
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "10MBy Installer",
        native_options,
        Box::new(|_cc| Ok(Box::new(SetupApp::new()))),
    )
    .expect("Failed to launch setup");
}
