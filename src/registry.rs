use std::env;
use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

const ALL_EXTENSIONS: &[&str] = &[
    ".mp4", ".mkv", ".avi", ".mov", ".webm", ".wmv", ".flv", ".m4v", ".mp3", ".wav", ".flac",
    ".aac", ".ogg", ".m4a", ".wma",
];

/// Check if context menu entries are already installed.
pub fn is_installed() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(
        "Software\\Classes\\SystemFileAssociations\\{}\\shell\\10MBy",
        ALL_EXTENSIONS[0]
    );
    hkcu.open_subkey(&key_path).is_ok()
}

/// Install context menu entries for all supported file extensions.
/// Adds "Compress to 10 MB" to the right-click menu on Windows.
pub fn install() -> Result<(), String> {
    let exe_path = env::current_exe().map_err(|e| format!("Cannot get exe path: {e}"))?;
    let exe_dir = exe_path.parent().ok_or("Cannot get exe directory")?;

    // Add exe directory to user PATH
    add_to_path(exe_dir)?;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    for ext in ALL_EXTENSIONS {
        let key_path = format!("Software\\Classes\\SystemFileAssociations\\{ext}\\shell\\10MBy");

        // Create the shell entry
        if let Ok(key) = hkcu.create_subkey(&key_path) {
            let _ = key.0.set_value("", &"Compress to 10 MB");
            let _ = key
                .0
                .set_value("Icon", &exe_path.to_string_lossy().to_string());
        }

        // Create the command entry
        let cmd_path = format!("{key_path}\\command");
        if let Ok(cmd_key) = hkcu.create_subkey(&cmd_path) {
            let command = format!("\"{}\" \"%1\"", exe_path.to_string_lossy());
            let _ = cmd_key.0.set_value("", &command);
        }
    }

    println!("10MBy installed! Right-click any supported media file to compress.");
    Ok(())
}

/// Remove all context menu entries and PATH modification.
pub fn uninstall() -> Result<(), String> {
    let exe_path = env::current_exe().map_err(|e| format!("Cannot get exe path: {e}"))?;
    let exe_dir = exe_path.parent().ok_or("Cannot get exe directory")?;

    // Remove from PATH
    remove_from_path(exe_dir)?;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    for ext in ALL_EXTENSIONS {
        let key_path =
            format!("Software\\Classes\\SystemFileAssociations\\{ext}\\shell\\10MBy\\command");
        let _ = hkcu.delete_subkey_all(&key_path);

        let key_path = format!("Software\\Classes\\SystemFileAssociations\\{ext}\\shell\\10MBy");
        let _ = hkcu.delete_subkey_all(&key_path);
    }

    println!("10MBy uninstalled.");
    Ok(())
}

fn add_to_path(exe_dir: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .map_err(|e| format!("Cannot open Environment key: {e}"))?;

    let current_path: String = env_key.get_value("Path").unwrap_or_default();

    let exe_str = exe_dir.to_string_lossy().to_string();

    if !current_path.split(';').any(|p| p.trim() == exe_str) {
        let new_path = if current_path.is_empty() {
            exe_str
        } else {
            format!("{current_path};{exe_str}")
        };
        env_key
            .set_value("Path", &new_path)
            .map_err(|e| format!("Cannot update PATH: {e}"))?;
    }

    Ok(())
}

fn remove_from_path(exe_dir: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .map_err(|e| format!("Cannot open Environment key: {e}"))?;

    let current_path: String = env_key.get_value("Path").unwrap_or_default();

    let exe_str = exe_dir.to_string_lossy().to_string();

    let paths: Vec<&str> = current_path
        .split(';')
        .map(|p| p.trim())
        .filter(|p| *p != exe_str && !p.is_empty())
        .collect();

    env_key
        .set_value("Path", &paths.join(";"))
        .map_err(|e| format!("Cannot update PATH: {e}"))?;

    Ok(())
}
