use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;

pub enum CopyingDecision {
    /// Do not copy the file, Remove it.
    DontCopy,
    /// Ignore the file, do not update.
    Ignore,
    /// Copy the file as is. (Maybe symbolic link)
    Passthrough,
    /// Copy the modified content.
    TextRewriting(String)
}

pub fn copy_decision_default(relative: &Path, src: &Path, dst: &Path) -> CopyingDecision {
    // compare the modification times - only copy if source is newer than target
    let src_path = src.join(relative);
    let dst_path = dst.join(relative);
    if !dst_path.exists() {
        return CopyingDecision::Passthrough;
    } else {
        match requires_update(&src_path, &dst_path) {
            Ok(true) => CopyingDecision::Passthrough,
            Ok(false) => CopyingDecision::Ignore,
            Err(_) => CopyingDecision::Passthrough
        }
    }
}

/// Generate a symbolic link.
/// If any file exists on the destination, this removes the file.
fn gen_symbolic_link(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_file(dst)?;
    }
    let src = std::path::absolute(src)?;
    let dst = std::path::absolute(dst)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst)?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(src, dst)?;
    Ok(())
}

/// Compare the modification dates of the source and destination files to determine if an update is required.
/// If the destination file is a symbolic link, this checks that the destination of the symlink equals to the `src` path.
fn requires_update(src: &Path, dst: &Path) -> Result<bool> {
    if !dst.exists() {
        return Ok(true);
    }
    let src_metadata = fs::metadata(src)?;
    let dst_metadata = fs::metadata(dst)?;
    Ok(src_metadata.modified()? > dst_metadata.modified()?)
}

/// Make the file readonly.
fn set_readonly(path: &Path, readonly: bool) -> Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_readonly(readonly);
    fs::set_permissions(path, perms)?;
    Ok(())
}

/// Clone the project.
/// `src` is the source project path.
/// `dst_origin` is the path of the destination**s**.
/// The destination path is the concat of `dst_origin` and `dst_after`.
/// `template_origin` is the path of the template**s**.
/// The template path is the concat of `template_origin` and `template_after`.
/// `dont_delete` and `dont_copy` verify the file must not be deleted or copied.
/// The first argument is the relative path to be deleted/copied.
/// The second argument is the source origin path and the third is the destination origin path.
///
/// ## Implementation
/// This function does not delete/copy `$dst/Cargo.toml`, `$dst/Cargo.lock`, and `$dst/target`.
/// ### Deletion
/// This 
pub fn clone_project<F1 : Fn(&Path, &Path, &Path) -> bool + ?Sized, F2: Fn(&Path, &Path, &Path) -> CopyingDecision + ?Sized>(src: &Path, dst_origin: &Path, dst_after: &Path, template_origin: &Path, template_after: &Path,  dont_delete : &F1, copy_decision : &F2) -> Result<()> {
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
            let entry = entry?; // The error seems fatal.
            let path = entry.path();

            let relative = path.strip_prefix(&dst)?; // If it throws an error, dst may a symbolic link?
            if src.join(relative).exists() || template_src_path.join(relative).exists() { // The source files exists, do not delete it.
                continue; // Ignored file.
            }
            if entry.file_type().is_dir() {
                fs::remove_dir_all(path)?;
            } else {
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
                    && !e.path().starts_with(dst_origin) // The file is in the destinations.
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
                if is_conflict(relative) {
                    continue; // Skip copying the file to avoid overwriting existing files in the destination.
                }

                match copy_decision(relative, src, &dst) {
                    CopyingDecision::DontCopy => {
                        if target_path.exists() {
                            fs::remove_file(&target_path)?
                        }
                        continue;
                    }
                    CopyingDecision::Ignore => continue,
                    CopyingDecision::Passthrough =>
                        if gen_symbolic_link(path, &target_path).is_err() { // Try to create a symbolic link first. If it fails, copy the file instead.
                            fs::copy(path, &target_path)
                                .with_context(|| format!("Failed to copy {:?}", path))?;
                        },
                    CopyingDecision::TextRewriting(new_content) => {
                        if target_path.exists() {
                            let md = target_path.symlink_metadata()?;
                            if md.is_symlink() ||  // Remove if the target is a symbolic link.
                                set_readonly(&target_path, false).is_err() { // Make the file writable before writing.
                                    println!("Removing file {:?}", target_path);
                                fs::remove_file(&target_path)?; 
                            }
                        }
                        fs::write(&target_path, new_content)
                            .with_context(|| format!("Failed to write {:?}", target_path))?
                    }
                }
                set_readonly(&target_path, true)?;
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
