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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    print!("\x1B[2J\x1B[1;1H");
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
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
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

fn clean_history() {
    println!("Start Cleaning");
    let termux_history = Path::new(TERMUX_ROOT).join("files/home/.bash_history");
    let debian_history = Path::new(TERMUX_ROOT).join("files/home/debian-fs/root/.bash_history");
    if termux_history.is_file() {
        println!("Clean Termux Bash History");
        let _ = fs::remove_file(&termux_history);
    }
    if debian_history.is_file() {
        println!("Clean Debian Bash History");
        let _ = fs::remove_file(&debian_history);
    }
}

fn append_dir_filtered<W: Write>(
    tar: &mut Builder<W>,
    archive_prefix: &Path,
    src_dir: &Path,
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
            append_dir_filtered(tar, &dst_path, &src_path)?;
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

    println!("Will Create {}", name);
    clean_history();
    println!("Backing System Up...");
    sleep_seconds(5);

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

    let file = fs::File::create(&backup_path)?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(enc);
    tar.follow_symlinks(false);

    append_dir_filtered(&mut tar, Path::new("home"), &home_dir)?;
    append_dir_filtered(&mut tar, Path::new("usr"), &usr_dir)?;

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

    let backup_path = backup_dir.join(&file);
    verify_sha256_file(&backup_path)?;

    println!("Start Restoring!");
    sleep_seconds(5);

    let files_dir = Path::new(TERMUX_ROOT).join("files");
    remove_dir_contents(&files_dir)?;

    let file = fs::File::open(&backup_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive.set_preserve_permissions(true);
    archive.set_unpack_xattrs(true);
    archive.unpack(&files_dir)?;

    println!("\x1B[0;32m Restoring Finished! \x1B[0m");
    Ok(())
}

fn remove_path(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn remove_dir_contents(dir: &Path) -> io::Result<()> {
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
}
