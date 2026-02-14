# Zeditor

A video editor built in Rust using [iced](https://iced.rs) for the GUI and FFmpeg 8 for media handling. Aims to become a feature-complete, crash-resistant alternative to existing video editors.

## Features

- Timeline with multi-track video and audio support
- Drag-and-drop clips from source library to timeline
- Cut (blade tool), move, resize, and snap clips
- Linked video+audio clip editing
- Undo/redo for all timeline operations
- Real-time video preview with playback
- Vertical/rotated video support
- Video rendering/export
- Project save/load

## Prerequisites

- **Rust 1.85+** (2024 edition)
- **FFmpeg 8** development libraries
- **pkg-config**
- **ALSA** development libraries (Linux audio via rodio)

## Installation

### Arch Linux

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install FFmpeg 8 and build dependencies
sudo pacman -S ffmpeg pkg-config alsa-lib

# Clone and build
git clone https://github.com/diericx/zeditormktwo.git
cd zeditormktwo
cargo build --release
```

### Ubuntu 24.04+

Ubuntu 24.04 ships FFmpeg 6.x by default. You need FFmpeg 8 libraries. The easiest approach is to use a PPA or build from source.

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install build tools and dependencies
sudo apt update
sudo apt install build-essential pkg-config libclang-dev \
    libasound2-dev libx264-dev nasm
```

**Option A: Build FFmpeg 8 from source**

```bash
# Download and build FFmpeg 8
wget https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz
tar xf ffmpeg-8.0.1.tar.xz
cd ffmpeg-8.0.1

./configure \
    --prefix=/usr/local \
    --enable-shared \
    --enable-gpl \
    --enable-libx264 \
    --disable-static

make -j$(nproc)
sudo make install
sudo ldconfig
cd ..
```

**Option B: Use a PPA (if available)**

Check for a community PPA that provides FFmpeg 8 packages. The key packages needed are `libavformat-dev`, `libavcodec-dev`, `libavutil-dev`, `libswscale-dev`, and `libswresample-dev` at version 8.x.

**Then build Zeditor:**

```bash
# Ensure pkg-config can find FFmpeg 8
export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:$PKG_CONFIG_PATH"
export LD_LIBRARY_PATH="/usr/local/lib:$LD_LIBRARY_PATH"

git clone https://github.com/diericx/zeditormktwo.git
cd zeditormktwo
cargo build --release
```

## Running

```bash
cargo run --release -p zeditor-ui --bin zeditor
```

Or run the built binary directly:

```bash
./target/release/zeditor
```

## Running Tests

Tests require FFmpeg 8 installed and the `ffmpeg` CLI available in `$PATH` (for generating test fixtures).

```bash
cargo test --workspace           # All tests
cargo test -p zeditor-core       # Core domain logic only
cargo test -p zeditor-media      # FFmpeg integration tests
cargo test -p zeditor-ui         # UI tests
```

## Architecture

Four-crate workspace with strict dependency layering:

```
zeditor-ui          Binary + iced GUI
  ├── zeditor-core  Pure domain logic (timeline, clips, undo/redo)
  └── zeditor-media FFmpeg integration (decode, probe, render, thumbnails)
        └── zeditor-core

zeditor-test-harness  Dev-only test builders and fixture generation
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| A | Arrow tool (select/drag/resize) |
| B | Blade tool (cut clips) |
| Delete / Backspace | Delete selected clip or source asset |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Space | Play/Pause |

## Troubleshooting

**`ffmpeg` not found / wrong version**

Verify FFmpeg 8 is installed:
```bash
ffmpeg -version  # Should show "ffmpeg version 8.x"
pkg-config --modversion libavformat  # Should show "62.x" (FFmpeg 8)
```

**Linker errors about FFmpeg symbols**

Make sure `pkg-config` can find the FFmpeg 8 libraries:
```bash
export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:$PKG_CONFIG_PATH"
```

**Runtime: "cannot open shared object file"**

Add the FFmpeg library path to your linker search path:
```bash
export LD_LIBRARY_PATH="/usr/local/lib:$LD_LIBRARY_PATH"
```

Or create a permanent config:
```bash
echo "/usr/local/lib" | sudo tee /etc/ld.so.conf.d/ffmpeg.conf
sudo ldconfig
```

## License

See LICENSE file for details.
