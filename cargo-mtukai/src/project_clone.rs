use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;

fn gen_symbolic_link(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_file(dst)?;
    }
    let src = src.canonicalize()?;
    let dst = dst.canonicalize()?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst)?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(src, dst)?;
    Ok(())
}

/// Compare the modification times of the source and destination files to determine if an update is required.
fn requires_update(src: &Path, dst: &Path) -> Result<bool> {
    if !dst.exists() {
        return Ok(true);
    }
    let src_metadata = fs::metadata(src)?;
    let dst_metadata = fs::metadata(dst)?;
    if dst_metadata.is_symlink() {
        let symlink_target = fs::read_link(dst)?;
        let src_canon = src.canonicalize()?;
        let target_canon = symlink_target.canonicalize()?;
        if src_canon == target_canon {
            return Ok(false);
        }
    }
    Ok(src_metadata.modified()? > dst_metadata.modified()?)
}

fn set_readonly(path: &Path, readonly: bool) -> Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_readonly(readonly);
    fs::set_permissions(path, perms)?;
    Ok(())
}

pub fn clone_project(src: &Path, dst_origin: &Path, dst_after: &Path, template_origin: &Path, template_after: &Path,  dont_delete : fn(&Path, &Path, &Path) -> bool, dont_copy : fn(&Path, &Path, &Path) -> bool) -> Result<()> {
    let template_src_path = template_origin.join(template_after);
    let dst = dst_origin.join(dst_after);
    let blacklist = [dst.join("Cargo.toml"), dst.join("Cargo.lock"), dst.join("target")];
    let file_filter = |e: &walkdir::DirEntry| {
        let path = e.path();
        !blacklist.iter().any(|p| path.starts_with(p)) // The file is not in the blacklist.
        && !dont_delete(path, src, &dst) // The file is not ignored.
    };
    if dst.exists() {
        for entry in
            WalkDir::new(&dst)
            .into_iter()
            .filter_entry(file_filter) {
            let entry = entry?;
            let path = entry.path();

            let relative = path.strip_prefix(&dst)?;
            if src.join(relative).exists() || template_src_path.join(relative).exists() { // The source files exists, do not delete it.
                continue; // Ignored file.
            }
            if entry.file_type().is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                set_readonly(path, false)?;
                fs::remove_file(path)?;
            }
        }
    }

    let mut blacklist = vec![src.join("Cargo.toml"), src.join("Cargo.lock"), src.join("target"), template_origin.to_path_buf()];
    let do_clone = |src : &Path, blacklist: &Vec<PathBuf>, is_conflict : &dyn Fn(&Path) -> bool| -> Result<()> {
        for entry in WalkDir::new(src)
                .into_iter()
                .filter_entry(|e| {
                    let path = e.path();
                    !blacklist.iter().any(|p| path.starts_with(p)) // The file is not in the blacklist.
                    && !dont_copy(path, src, &dst) // The file is not ignored.
                    && !e.path().starts_with(dst_origin)
                }) {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(src)?;
            let target_path = dst.join(relative);

            if path == target_path {
                continue; // Avoid copying the destination directory into itself.
            }

            if entry.file_type().is_dir() {
                fs::create_dir_all(&target_path)?;
            } else {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let target_exists = target_path.exists();
                // Compare modification times - only copy if source is newer than target
                let should_copy = target_exists && requires_update(path, &target_path)?;

                // If conflicting, skip copying the file to avoid overwriting existing files in the destination.
                if (!target_exists || should_copy) && !is_conflict(relative) {
                    if should_copy {
                        set_readonly(&target_path, false)?;
                    }
                    if target_path.extension() == Some(OsStr::new("rs")) // Rust source files are not copied as a symbolic link.
                    || gen_symbolic_link(path, &target_path).is_err(){ // If not, try to create a symbolic link.
                        fs::copy(path, &target_path) // If symbolic link creation fails, copy the file instead.
                            .with_context(|| format!("Failed to copy {:?}", path))?;
                    }
                    set_readonly(&target_path, true)?;
                }
            }
        }
        Ok(())
    };

    // Clone the original project.
    do_clone(src, &blacklist, &|relative| template_src_path.join(relative).exists())?;
    blacklist.pop(); // Remove the template directory from the blacklist.
    // Clone the template directory.
    if template_src_path.exists() {
        do_clone(&template_src_path, &blacklist, &|_| false)?;
    }
    Ok(())
}