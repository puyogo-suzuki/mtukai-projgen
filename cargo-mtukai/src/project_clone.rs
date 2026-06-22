use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;


pub fn clone_project(src: &Path, dst_origin: &Path, dst: &Path, dont_delete : fn(&Path, &Path, &Path) -> bool, dont_copy : fn(&Path, &Path, &Path) -> bool) -> Result<()> {
    let blacklist = [dst.join("Cargo.toml"), dst.join("Cargo.lock"), dst.join("target")];
    let file_filter = |e: &walkdir::DirEntry| {
        let path = e.path();
        !blacklist.iter().any(|p| path.starts_with(p)) // The file is not in the blacklist.
        && !dont_delete(path, src, dst) // The file is not ignored.
    };
    if dst.exists() {
        for entry in
            WalkDir::new(dst)
            .into_iter()
            .filter_entry(file_filter) {
            let entry = entry?;
            let path = entry.path();

            if src.join(path.strip_prefix(dst)?).exists() { // The source files exists, do not delete it.
                continue; // Ignored file.
            }
            if entry.file_type().is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                let mut perms = fs::metadata(path)?.permissions();
                perms.set_readonly(false);
                fs::set_permissions(path, perms)?;
                fs::remove_file(path)?;
            }
        }
    }

    let blacklist = [src.join("Cargo.toml"), src.join("Cargo.lock"), src.join("target")];
    let file_filter = |e: &walkdir::DirEntry| {
        let path = e.path();
        !blacklist.iter().any(|p| path.starts_with(p)) // The file is not in the blacklist.
        && !dont_copy(path, src, dst) // The file is not ignored.
    };
    for entry in WalkDir::new(src)
        .into_iter()
        .filter_entry(|e| file_filter(e) && !e.path().starts_with(dst_origin)) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(src)?;
        let target_path = dst.join(relative);

        if path == dst {
            continue; // Avoid copying the destination directory into itself.
        }

        if entry.file_type().is_dir() {
            fs::create_dir_all(&target_path)?;
        } else {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Compare modification times - only copy if source is newer than target
            let should_copy = if target_path.exists() {
                let target_metadata = fs::metadata(&target_path)?;
                let source_metadata = fs::metadata(path)?;
                
                let target_modified = target_metadata.modified()?;
                let source_modified = source_metadata.modified()?;
                
                let res = source_modified > target_modified;
                if res {
                    let mut perm = target_metadata.permissions();
                    perm.set_readonly(false);
                    fs::set_permissions(&target_path, perm)?;
                }
                res
            } else {
                true
            };

            if should_copy {
                fs::copy(path, &target_path)
                    .with_context(|| format!("Failed to copy {:?}", path))?;

                let mut perms = fs::metadata(&target_path)?.permissions();
                perms.set_readonly(true);
                fs::set_permissions(&target_path, perms)?;
            }
        }
    }
    Ok(())
}