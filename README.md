# termux-backup
A Rust program to backup and restore your Termux home and sysroot directories.

## Download Counter
<p align="left">
<a href="https://github.com/rc-chuah/termux-backup/releases/latest"><img alt="GitHub all releases" src="https://img.shields.io/github/downloads/rc-chuah/termux-backup/total?label=GITHUB%20DOWNLOADS&style=for-the-badge&color=brightgreen"></a>
<a href="https://crates.io/crates/termux-backup"><img alt="Crates.io - Downloads" src="https://img.shields.io/crates/d/termux-backup?label=CRATES.IO%20DOWNLOADS&style=for-the-badge&color=brightgreen"></a>
</p>

## Usage
### How To Use termux-backup

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
   pkg update -y && pkg install git rust tar gzip -y
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
5. Install to PATH
   ```bash
   cp target/release/termux-backup $PREFIX/bin/termux-backup && chmod +x $PREFIX/bin/termux-backup
   ```
6. Run the program:
   ```bash
   termux-backup
   ```

### Notes
- Backup only works in a normal Termux session.
- Restore works in both normal and failsafe sessions.
- Backups are stored at `/storage/emulated/0/termux/backups`.
- The restore process in a normal session will clean the current Termux environment before restoring, keeping only essential core utilities needed for extraction.

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
