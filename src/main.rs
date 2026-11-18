//! # `termux-backup`
//!
//! A fast, pure-Rust CLI utility to back up and restore Termux environments
//! (`/data/data/com.termux/files/home` and `usr`) on Android with live progress
//! reporting and SHA-256 verification.
//!
//! ## Overview
//!
//! Rewritten from Zongou's original Bash script into Rust, `termux-backup`
//! archives `/data/data/com.termux/files` (`home` and `usr`) into a
//! gzip-compressed TAR archive (`.tar.gz`) stored at
//! `/storage/emulated/0/termux/backups` alongside a matching `.sha256` checksum
//! file. During restoration, it verifies the archive against its checksum,
//! safely cleans existing files (including read-only cached directories), and
//! extracts the backup in pure Rust.
//!
//! ## Key Capabilities
//!
//! - **Pure-Rust Archiving & Extraction:** Built on [`tar`] and [`flate2`] with
//!   zero external `tar` or `gzip` binary dependencies.
//! - **Interactive Terminal Menu:** Guides users through storage permission
//!   checks, backup directory setup, backup creation, and archive restoration.
//! - **Session Detection:** Distinguishes between `[NORMAL SESSION]` (where
//!   both backup and restore are permitted) and `[FAILSAFE SESSION]` (where
//!   only restore is allowed to prevent incomplete backups).
//! - **SHA-256 Integrity & Path Safety:** Generates and validates `.sha256`
//!   checksums before modifying the Termux environment, prevents path traversal
//!   via strict filename validation, and skips special Unix device, socket, and
//!   FIFO files when archiving.
//! - **Non-Destructive Backups:** Preserves user files and shell history
//!   (`.bash_history`) intact without modifying `$HOME` during backup.
//! - **Permission-Resilient Cleanup:** Automatically adjusts read-only
//!   directory permissions (`0o700`, such as Go module or Cargo caches) before
//!   wiping existing Termux directories during restore.
//! - **Mobile-Safe Progress Tracking:** Displays live transfer progress bars,
//!   transferred/total byte sizes (`MB`/`GB`), throughput (`MB/s`), and
//!   estimated time remaining (`ETA`) formatted cleanly for narrow Termux
//!   portrait terminals.
//!
//! ## Installation
//!
//! ```sh
//! cargo install --locked termux-backup
//! ```
//!
//! ## Usage
//!
//! Run the compiled binary directly inside Termux:
//!
//! ```sh
//! termux-backup
//! ```
//!
//! Follow the on-screen prompts to select:
//! 1. **Backup** — Archive `home` and `usr` into `/storage/emulated/0/termux/backups` (defaults to `termux-YYYYMMDD-HHMMSS.tar.gz` in UTC).
//! 2. **Restore** — Verify `.sha256` checksum, clean `/data/data/com.termux/files`, and unpack a chosen archive back into Termux.
//! 3. **Exit Menu** — Quit the program.

// termux-backup - Rust rewrite
// Original bash script by Zongou
// Rewritten in Rust by RC Chuah-(RaynerSec)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Write as _;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
use tar::Archive;
use tar::Builder;

const BACKUP_DIR: &str = "/storage/emulated/0/termux/backups";
const TERMUX_ROOT: &str = "/data/data/com.termux";

#[derive(Debug, Clone, Copy, PartialEq)]
enum SessionMode {
    Normal,
    Failsafe,
}

fn format_size(bytes: u64) -> String {
    const MB: f64 = 1_048_576.0;
    const GB: f64 = 1_073_741_824.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.1} MB", b / MB)
    }
}

struct ProgressBar {
    label: &'static str,
    total_bytes: u64,
    current_bytes: u64,
    start: Instant,
    last_draw: Instant,
    drawn: bool,
}

impl ProgressBar {
    fn new(label: &'static str, total_bytes: u64) -> Self {
        let now = Instant::now();
        let mut bar = Self {
            label,
            total_bytes,
            current_bytes: 0,
            start: now,
            last_draw: now - Duration::from_secs(1),
            drawn: false,
        };
        bar.render(true);
        bar
    }

    fn advance(&mut self, bytes: u64) {
        self.current_bytes = self.current_bytes.saturating_add(bytes);
        if self.last_draw.elapsed() >= Duration::from_millis(100) {
            self.render(false);
        }
    }

    fn finish(&mut self) {
        if self.total_bytes > 0 {
            self.current_bytes = self.total_bytes;
        }
        self.render(true);
        println!();
    }

    fn render(&mut self, force: bool) {
        if !force && self.last_draw.elapsed() < Duration::from_millis(100) {
            return;
        }
        self.last_draw = Instant::now();

        let elapsed = self.start.elapsed().as_secs_f64().max(0.001);
        let speed_mb = (self.current_bytes as f64 / 1_048_576.0) / elapsed;

        if self.drawn {
            print!("\r\x1B[2K\x1B[1A\r\x1B[2K");
        }
        self.drawn = true;

        if self.total_bytes > 0 {
            let clamped = self.current_bytes.min(self.total_bytes);
            let ratio = clamped as f64 / self.total_bytes as f64;
            let pct = (ratio * 100.0).min(100.0);
            let width = 18usize;
            let filled = ((ratio * width as f64).round() as usize).min(width);
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(width - filled));

            let eta_secs = if clamped > 0 && clamped < self.total_bytes {
                let remaining = (self.total_bytes - clamped) as f64;
                let rate = clamped as f64 / elapsed;
                (remaining / rate).round() as u64
            } else {
                0
            };

            println!("{}: [{}] {:5.1}%", self.label, bar, pct);
            print!(
                "{}/{} | {:.1} MB/s | ETA {:02}:{:02}",
                format_size(clamped),
                format_size(self.total_bytes),
                speed_mb,
                eta_secs / 60,
                eta_secs % 60
            );
        } else {
            println!("{}: {}", self.label, format_size(self.current_bytes));
            print!("Speed: {:.1} MB/s", speed_mb);
        }
        let _ = io::stdout().flush();
    }
}

struct ProgressReader<'a, R: Read> {
    inner: R,
    progress: &'a mut ProgressBar,
}

impl<'a, R: Read> ProgressReader<'a, R> {
    fn new(inner: R, progress: &'a mut ProgressBar) -> Self {
        Self { inner, progress }
    }
}

impl<R: Read> Read for ProgressReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.progress.advance(n as u64);
        }
        Ok(n)
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    loop {
        clear_screen();
        check_storage_permission();
        clear_screen();
        check_backup_dir()?;
        clear_screen();
        let session = session_mode();
        println!("This Program Will Backup/Restore Termux");
        println!("Choose To Backup, Restore Or Exit Menu");
        println!("1 To Backup, 2 To Restore, 3 To Exit Menu");
        println!();
        print!("Enter Option: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;
        let option = input.trim();
        match option {
            "1" => {
                clear_screen();
                backup(session)?;
                press_enter()?;
            }
            "2" => {
                clear_screen();
                restore()?;
                press_enter()?;
            }
            "3" => {
                clear_screen();
                break;
            }
            _ => {
                clear_screen();
                incorrect_selection();
                press_enter()?;
            }
        }
    }
    Ok(())
}

fn clear_screen() {
    print!("\x1B[2J\x1B[3J\x1B[H");
    let _ = io::stdout().flush();
}

fn sleep_seconds(seconds: u64) {
    thread::sleep(Duration::from_secs(seconds));
}

fn is_safe_filename(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn format_utc_timestamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let time_of_day = unix_secs % 86_400;
    let hour = time_of_day / 3_600;
    let minute = (time_of_day % 3_600) / 60;
    let second = time_of_day % 60;

    // Howard Hinnant's civil-from-days algorithm (UTC)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if month <= 2 { 1 } else { 0 };

    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        year, month, day, hour, minute, second
    )
}

fn default_backup_filename() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("termux-{}.tar.gz", format_utc_timestamp(secs))
}

fn resolve_backup_filename(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        default_backup_filename()
    } else if trimmed.ends_with(".tar.gz") || trimmed.ends_with(".tgz") {
        trimmed.to_string()
    } else {
        format!("{}.tar.gz", trimmed)
    }
}

fn compute_sha256(path: &Path) -> io::Result<String> {
    let total_bytes = fs::metadata(path)?.len();
    let file = fs::File::open(path)?;
    let mut progress = ProgressBar::new("SHA256", total_bytes);
    let mut reader = ProgressReader::new(file, &mut progress);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    progress.finish();
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut hex, "{:02x}", byte);
    }
    Ok(hex)
}

fn write_sha256_file(backup_path: &Path, filename: &str) -> io::Result<String> {
    let hash = compute_sha256(backup_path)?;
    let sha_path = PathBuf::from(format!("{}.sha256", backup_path.display()));
    fs::write(&sha_path, format!("{}  {}\n", hash, filename))?;
    Ok(hash)
}

fn verify_sha256_file(backup_path: &Path) -> io::Result<()> {
    let sha_path = PathBuf::from(format!("{}.sha256", backup_path.display()));
    if !sha_path.is_file() {
        println!("No .sha256 checksum file found, skipping verification.");
        return Ok(());
    }
    println!("Verifying SHA256 Checksum...");
    let content = fs::read_to_string(&sha_path)?;
    let expected = content
        .split_whitespace()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid .sha256 file format"))?;
    let actual = compute_sha256(backup_path)?;
    if actual.eq_ignore_ascii_case(expected) {
        println!("\x1B[0;32m SHA256 Checksum Verified! \x1B[0m");
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "SHA256 checksum mismatch! Expected {}, got {}",
                expected, actual
            ),
        ))
    }
}

fn command_exists(cmd: &str) -> bool {
    Command::new("/data/data/com.termux/files/usr/bin/sh")
        .arg("-c")
        .arg(format!("command -v {}", cmd))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn session_mode() -> SessionMode {
    if command_exists("termux-info") {
        println!("[NORMAL SESSION]");
        SessionMode::Normal
    } else {
        println!("[FAILSAFE SESSION]");
        SessionMode::Failsafe
    }
}

fn storage_writable() -> bool {
    Command::new("/data/data/com.termux/files/usr/bin/sh")
        .arg("-c")
        .arg("[ -w /storage/emulated/0 ]")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn check_storage_permission() {
    if !storage_writable() {
        println!("Setting Up Termux Storage...");
        let _ = Command::new("/data/data/com.termux/files/usr/bin/termux-setup-storage")
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        sleep_seconds(5);
        println!("Setting Up Termux Storage Completed...");
        sleep_seconds(5);
    }
}

fn check_backup_dir() -> io::Result<()> {
    let backup_dir = Path::new(BACKUP_DIR);
    if backup_dir.exists() {
        return Ok(());
    }
    println!("Backup Directory:");
    println!("{}", BACKUP_DIR);
    println!("Backup Directory Does Not Exists");
    println!();
    print!("Create Backup Directory? [y/n] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    if answer.trim().eq_ignore_ascii_case("y") {
        println!("Creating Backup Directory...");
        fs::create_dir_all(backup_dir).map_err(|e| {
            io::Error::new(e.kind(), format!("Create Backup Directory Failed! {}", e))
        })?;
        sleep_seconds(5);
        println!("Creating Backup Directory Completed...");
        sleep_seconds(5);
        Ok(())
    } else {
        Err(io::Error::other("Backup directory creation declined"))
    }
}

fn calculate_dir_bytes(src_dir: &Path) -> io::Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(src_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();

        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if file_type.is_socket()
                || file_type.is_fifo()
                || file_type.is_block_device()
                || file_type.is_char_device()
            {
                continue;
            }
        }

        if file_type.is_dir() {
            total = total.saturating_add(calculate_dir_bytes(&src_path)?);
        } else if file_type.is_file() {
            if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Ok(total)
}

fn append_dir_filtered<W: Write>(
    tar: &mut Builder<W>,
    archive_prefix: &Path,
    src_dir: &Path,
    progress: &mut ProgressBar,
) -> io::Result<()> {
    tar.append_dir(archive_prefix, src_dir)?;
    for entry in fs::read_dir(src_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = archive_prefix.join(entry.file_name());

        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if file_type.is_socket()
                || file_type.is_fifo()
                || file_type.is_block_device()
                || file_type.is_char_device()
            {
                continue;
            }
        }

        if file_type.is_dir() {
            append_dir_filtered(tar, &dst_path, &src_path, progress)?;
        } else if file_type.is_file() {
            let meta = fs::symlink_metadata(&src_path)?;
            let mut header = tar::Header::new_gnu();
            header.set_metadata(&meta);
            header.set_cksum();
            let file = fs::File::open(&src_path)?;
            let mut reader = ProgressReader::new(file, progress);
            tar.append_data(&mut header, &dst_path, &mut reader)?;
        } else {
            tar.append_path_with_name(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn backup(session: SessionMode) -> io::Result<()> {
    if session != SessionMode::Normal {
        println!("Backup Is Not Supported In [FAILSAFE SESSION], Exiting...");
        return Ok(());
    }
    println!("Type The File Name For Backup");
    println!("If Empty Will Use termux-YYYYMMDD-HHMMSS.tar.gz");
    println!();
    print!("Enter File Name For Backup: ");
    io::stdout().flush()?;
    let mut name = String::new();
    io::stdin().lock().read_line(&mut name)?;
    let name = resolve_backup_filename(&name);

    if !is_safe_filename(&name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid backup filename: must not contain path separators or '..'",
        ));
    }

    clear_screen();
    println!("Will Create {}", name);
    println!("Scanning Files...");

    let backup_path = Path::new(BACKUP_DIR).join(&name);
    let files_dir = Path::new(TERMUX_ROOT).join("files");
    let home_dir = files_dir.join("home");
    let usr_dir = files_dir.join("usr");

    if !home_dir.exists() || !usr_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Make Sure Running In Termux Default Environment.",
        ));
    }

    let total_bytes =
        calculate_dir_bytes(&home_dir)?.saturating_add(calculate_dir_bytes(&usr_dir)?);
    println!("Backing System Up...");

    let file = fs::File::create(&backup_path)?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(enc);
    tar.follow_symlinks(false);

    let mut progress = ProgressBar::new("Backup", total_bytes);
    append_dir_filtered(&mut tar, Path::new("home"), &home_dir, &mut progress)?;
    append_dir_filtered(&mut tar, Path::new("usr"), &usr_dir, &mut progress)?;
    progress.finish();

    let enc = tar.into_inner()?;
    enc.finish()?;

    println!("Generating SHA256 Checksum...");
    let hash = write_sha256_file(&backup_path, &name)?;
    println!("SHA256: {}", hash);

    println!("\x1B[0;32m Backing Up Finished! \x1B[0m");
    Ok(())
}

fn list_backup_files(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.ends_with(".sha256") {
                            println!("{}", name);
                        }
                    }
                }
            }
        }
    }
}

fn restore() -> io::Result<()> {
    let backup_dir = Path::new(BACKUP_DIR);

    let has_files = fs::read_dir(backup_dir)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                e.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                    && !e.file_name().to_string_lossy().ends_with(".sha256")
            })
        })
        .unwrap_or(false);

    if !has_files {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Backup Directory Is Empty! Exiting...",
        ));
    }

    println!("Note: Make Sure No Background Program Running.");
    println!("Listing Backup File...");
    println!();
    list_backup_files(backup_dir);
    println!();
    print!("Choose A Backup File: ");
    io::stdout().flush()?;
    let mut file = String::new();
    io::stdin().lock().read_line(&mut file)?;
    let mut file = file.trim().to_string();

    while !is_safe_filename(&file) || file.ends_with(".sha256") || !backup_dir.join(&file).is_file()
    {
        clear_screen();
        println!("No Match, Try Again!");
        sleep_seconds(5);
        clear_screen();
        println!("Note: Make Sure No Background Program Running.");
        println!("Listing Backup File...");
        println!();
        list_backup_files(backup_dir);
        println!();
        print!("Choose A Backup File: ");
        io::stdout().flush()?;
        file.clear();
        io::stdin().lock().read_line(&mut file)?;
        file = file.trim().to_string();
    }

    clear_screen();
    let backup_path = backup_dir.join(&file);
    verify_sha256_file(&backup_path)?;

    println!("Start Restoring!");
    sleep_seconds(5);

    let files_dir = Path::new(TERMUX_ROOT).join("files");
    remove_dir_contents(&files_dir)?;

    let archive_bytes = fs::metadata(&backup_path)?.len();
    let file = fs::File::open(&backup_path)?;
    let mut progress = ProgressBar::new("Restore", archive_bytes);
    let reader = ProgressReader::new(file, &mut progress);
    let decoder = GzDecoder::new(reader);
    let mut archive = Archive::new(decoder);
    archive.set_preserve_permissions(true);
    archive.set_unpack_xattrs(true);
    archive.unpack(&files_dir)?;
    progress.finish();

    println!("\x1B[0;32m Restoring Finished! \x1B[0m");
    Ok(())
}

#[cfg(unix)]
fn make_dir_accessible(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = fs::symlink_metadata(dir) {
        let mode = meta.permissions().mode();
        if (mode & 0o700) != 0o700 {
            let _ = fs::set_permissions(dir, fs::Permissions::from_mode(mode | 0o700));
        }
    }
}

#[cfg(not(unix))]
fn make_dir_accessible(_dir: &Path) {}

fn remove_path(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        make_dir_accessible(path);
        remove_dir_contents(path)?;
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

fn remove_dir_contents(dir: &Path) -> io::Result<()> {
    make_dir_accessible(dir);
    for entry in fs::read_dir(dir)? {
        remove_path(&entry?.path())?;
    }
    Ok(())
}

fn press_enter() -> io::Result<()> {
    println!();
    print!("Press Enter To Continue...");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().lock().read_line(&mut buf)?;
    clear_screen();
    Ok(())
}

fn incorrect_selection() {
    println!("Incorrect Selection! Try Again.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_is_safe_filename_valid() {
        assert!(is_safe_filename("termux.tar.gz"));
        assert!(is_safe_filename("termux-20261116-152500.tar.gz"));
        assert!(is_safe_filename("backup-2026-11-16.tgz"));
        assert!(is_safe_filename("my_backup.tar.gz"));
    }

    #[test]
    fn test_is_safe_filename_rejects_traversal_and_paths() {
        assert!(!is_safe_filename(""));
        assert!(!is_safe_filename("."));
        assert!(!is_safe_filename(".."));
        assert!(!is_safe_filename("../termux.tar.gz"));
        assert!(!is_safe_filename("subdir/termux.tar.gz"));
        assert!(!is_safe_filename("/storage/emulated/0/termux.tar.gz"));
    }

    #[test]
    fn test_format_utc_timestamp() {
        assert_eq!(format_utc_timestamp(0), "19700101-000000");
        assert_eq!(format_utc_timestamp(1_700_000_000), "20231114-221320");
    }

    #[test]
    fn test_default_backup_filename() {
        let name = default_backup_filename();
        assert!(name.starts_with("termux-"));
        assert!(name.ends_with(".tar.gz"));
        assert_eq!(name.len(), "termux-YYYYMMDD-HHMMSS.tar.gz".len());
        assert!(is_safe_filename(&name));
    }

    #[test]
    fn test_resolve_backup_filename() {
        let empty = resolve_backup_filename("");
        assert!(empty.starts_with("termux-"));
        assert!(empty.ends_with(".tar.gz"));

        let whitespace = resolve_backup_filename("   \n");
        assert!(whitespace.starts_with("termux-"));
        assert!(whitespace.ends_with(".tar.gz"));

        assert_eq!(resolve_backup_filename("my-backup"), "my-backup.tar.gz");
        assert_eq!(
            resolve_backup_filename("my-backup.tar.gz"),
            "my-backup.tar.gz"
        );
        assert_eq!(resolve_backup_filename("my-backup.tgz"), "my-backup.tgz");
    }

    #[test]
    fn test_sha256_write_and_verify() {
        let dir = temp_test_dir("termux_backup_sha256");
        let archive = dir.join("test.tar.gz");
        fs::write(&archive, b"hello termux backup").unwrap();

        let hash = write_sha256_file(&archive, "test.tar.gz").unwrap();
        assert_eq!(hash.len(), 64);
        assert!(verify_sha256_file(&archive).is_ok());

        fs::write(&archive, b"corrupted data").unwrap();
        assert!(verify_sha256_file(&archive).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_progress_reader_tracks_bytes() {
        let payload = b"0123456789abcdef";
        let mut bar = ProgressBar::new("Test", payload.len() as u64);
        let mut reader = ProgressReader::new(&payload[..], &mut bar);
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        bar.finish();

        assert_eq!(out, payload);
        assert_eq!(bar.current_bytes, payload.len() as u64);
    }

    #[cfg(unix)]
    #[test]
    fn test_remove_dir_contents_with_readonly_subdir() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_test_dir("termux_backup_chmod_cleanup");
        let readonly_sub = dir.join("go_pkg_mod");
        fs::create_dir_all(&readonly_sub).unwrap();
        let nested_file = readonly_sub.join("module.go");
        fs::write(&nested_file, b"package mod").unwrap();

        // Simulate read-only directory (0500 / r-x------) like Go module caches
        fs::set_permissions(&readonly_sub, fs::Permissions::from_mode(0o500)).unwrap();

        remove_dir_contents(&dir).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);

        let _ = fs::remove_dir_all(&dir);
    }
}
