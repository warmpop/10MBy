use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Copy a file to the Windows clipboard using PowerShell.
/// This copies the actual file (not just the path), so the user can
/// paste it into any folder or chat application.
pub fn copy_file_to_clipboard(path: &Path) {
    let path_str = path.to_string_lossy().replace('"', "\"\"");

    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-Command",
        &format!("Set-Clipboard -Path \"{path_str}\""),
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let _ = cmd.status();
}
