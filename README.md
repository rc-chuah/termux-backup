# termux-backup

[![Build & Analyze](https://github.com/rc-chuah/termux-backup/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/rc-chuah/termux-backup/actions/workflows/build.yml)
[![Crates.io Version](https://img.shields.io/crates/v/termux-backup.svg)](https://crates.io/crates/termux-backup)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](https://github.com/rc-chuah/termux-backup/blob/main/LICENSE)

A Rust program to backup and restore your Termux home and sysroot directories.

## Download Counter
<p align="left">
<a href="https://github.com/rc-chuah/termux-backup/releases/latest"><img alt="GitHub all releases" src="https://img.shields.io/github/downloads/rc-chuah/termux-backup/total?label=GITHUB%20DOWNLOADS&style=for-the-badge&color=brightgreen"></a>
<a href="https://crates.io/crates/termux-backup"><img alt="Crates.io - Downloads" src="https://img.shields.io/crates/d/termux-backup?label=CRATES.IO%20DOWNLOADS&style=for-the-badge&color=brightgreen"></a>
</p>

## Usage
### How To Use termux-backup

#### Install from GitHub Releases (`.tar.gz`)
1. Download and extract the latest `.tar.gz` archive for your architecture:
   ```bash
   case "$(dpkg --print-architecture)" in
     aarch64) TARGET="aarch64-linux-android" ;;
     arm)     TARGET="armv7-linux-androideabi" ;;
     i686)    TARGET="i686-linux-android" ;;
     x86_64)  TARGET="x86_64-linux-android" ;;
   esac
   VERSION=$(curl -sL https://api.github.com/repos/rc-chuah/termux-backup/releases/latest | grep -Po '"tag_name": "\K[^"]*')
   TAR_VERSION="${VERSION#v}"
   curl -LO "https://github.com/rc-chuah/termux-backup/releases/download/${VERSION}/termux-backup-${TAR_VERSION}-${TARGET}.tar.gz"
   tar -xzf "termux-backup-${TAR_VERSION}-${TARGET}.tar.gz" -C "$PREFIX/bin"
   chmod +x "$PREFIX/bin/termux-backup"
   rm -f "termux-backup-${TAR_VERSION}-${TARGET}.tar.gz"
   ```
   Or download it directly from [GitHub Releases](https://github.com/rc-chuah/termux-backup/releases/latest), extract `termux-backup`, and move it to `$PREFIX/bin/`.
2. Run the program:
   ```bash
   termux-backup
   ```

#### Install from crates.io
1. Install using Cargo:
   ```bash
   cargo install --locked termux-backup
   ```
   Or install with latest compatible dependency versions:
   ```bash
   cargo install termux-backup
   ```
   Or install using [cargo-binstall](https://github.com/cargo-bins/cargo-binstall):
   ```bash
   cargo binstall termux-backup
   ```
2. Run the program:
   ```bash
   termux-backup
   ```

#### Build from source
1. Install dependencies in Termux:
   ```bash
   pkg update -y && pkg install git rust -y
   ```
2. Git clone the repository:
   ```bash
   git clone https://github.com/rc-chuah/termux-backup
   ```
3. Change directory:
   ```bash
   cd termux-backup
   ```
4. Build the release binary:
   ```bash
   cargo build --release
   ```
5. Install to PATH:
   ```bash
   cp target/release/termux-backup $PREFIX/bin/termux-backup && chmod +x $PREFIX/bin/termux-backup
   ```
6. Run the program:
   ```bash
   termux-backup
   ```

#### Verify Release Artifacts
All release `.tar.gz` archives include SHA256 checksums and [GitHub Artifact Attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations).

1. Verify build provenance using [GitHub CLI](https://github.com/cli/cli) (`pkg install gh` in Termux):
   ```bash
   # Verify .tar.gz archive
   gh attestation verify termux-backup-<version>-<target>.tar.gz --repo rc-chuah/termux-backup
   ```
2. Or verify the SHA256 checksum:
   ```bash
   # Verify .tar.gz checksum
   sha256sum -c termux-backup-<version>-<target>.tar.gz.sha256
   ```

### Notes
- Backup only works in a normal Termux session.
- Restore works in both normal and failsafe sessions.
- Backups are 100% non-destructive and preserve shell history files (`.bash_history`) intact.
- Backups and their `.sha256` checksum files are stored at `/storage/emulated/0/termux/backups`.
- When prompted for a backup name, leaving it blank defaults to `termux-YYYYMMDD-HHMMSS.tar.gz` (UTC), while custom names automatically append `.tar.gz` if `.tar.gz` or `.tgz` is omitted.
- SHA-256 checksum calculation, backup compression, and archive restoration display a live progress bar with percentage, `MB/s` speed, and ETA.
- During restore, the archive is automatically verified against its `.sha256` checksum file if present before modifying the Termux environment.
- During restore, the existing Termux `files` directory is cleaned before extracting the backup archive in pure Rust.

## Star History
<a href="https://www.star-history.com/#rc-chuah/termux-backup&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=rc-chuah/termux-backup&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=rc-chuah/termux-backup&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=rc-chuah/termux-backup&type=Date" />
 </picture>
</a>

## Credits
- Original bash script by Zongou.
- Rewritten in Rust by RC Chuah-(RaynerSec).

## License

Licensed under the [GNU General Public License v3.0](https://github.com/rc-chuah/termux-backup/blob/main/LICENSE).
test
