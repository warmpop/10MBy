use std::path::PathBuf;

#[cfg(windows)]
extern crate winres;

fn main() {
    // For the setup binary: ensure 10MBy.exe is built first so we can embed it
    let bin_name = std::env::var("CARGO_BIN_NAME").unwrap_or_default();
    if bin_name == "setup" {
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
        let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(&profile)
            .join("10MBy.exe");

        if !exe.exists() {
            let mut cmd = std::process::Command::new("cargo");
            cmd.args(["build", "--bin", "10MBy"]);
            if profile == "release" {
                cmd.arg("--release");
            }
            let status = cmd
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .status()
                .expect("Failed to build 10MBy.exe first");
            assert!(status.success(), "10MBy.exe build failed");
        }
    }

    // Embed Windows icon resource
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("10mby.ico");
        res.compile().unwrap();
    }
}
