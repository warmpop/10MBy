# 10MBy

[![Download](https://img.shields.io/badge/Download-v1.0-2ea44f?style=for-the-badge)](https://github.com/warmpop/10MBy/releases/download/release/10MBy-1.0-setup.exe)

Compress video and audio files to fit under 10 MB — right from the Windows context menu.

## Install

1. Download the latest release from [Releases](https://github.com/warmpop/10MBy/releases)
2. Run `setup.exe` and click **Install**

Setup will:
- Copy files to `%LOCALAPPDATA%\Programs\10MBy\`
- Create a Start Menu shortcut
- Add "Compress to 10 MB" to the right-click menu for supported files
- Install FFmpeg automatically if not already present

## Uninstall

- **From the app**: Click the &#x2699; gear icon → **Uninstall 10MBy**
- **From setup.exe**: Run it again — detects existing install and offers Uninstall

## Usage

- Right-click any video or audio file → **Compress to 10 MB**
- Or open 10MBy and drag a file in / click **Open File**
- The compressed file is saved next to the original and copied to your clipboard

## Supported Formats

**Video**: MP4, MKV, AVI, MOV, WEBM, WMV, FLV, M4V  
**Audio**: MP3, WAV, FLAC, AAC, OGG, M4A, WMA

## How It Works

10MBy uses **FFmpeg** under the hood. It probes the file duration, calculates the exact bitrate needed to fit under 10 MB, and applies the gentlest compression possible. If the estimate is slightly off, it steps down one more level — no brute-force retries, no wasted time.

## Build from Source

Requirements:
- [Rust](https://rustup.rs)
- [FFmpeg](https://ffmpeg.org) in PATH (or let setup.exe install it via winget)

```bash
git clone https://github.com/warmpop/10MBy.git
cd 10MBy
cargo build --release
```

Outputs in `target/release/`:
- `10MBy.exe` — main app
- `setup.exe` — installer / uninstaller

## License

MIT
