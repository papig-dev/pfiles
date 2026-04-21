use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub extension: Option<String>,
    pub is_hidden: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Drive {
    pub name: String,
    pub path: String,
}

fn to_entry(path: &Path) -> Option<FileEntry> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return None,
    };
    let is_symlink = meta.file_type().is_symlink();
    let resolved = fs::metadata(path).ok();
    let is_dir = resolved.as_ref().map(|m| m.is_dir()).unwrap_or(false);
    let size = resolved.as_ref().map(|m| m.len()).unwrap_or(0);
    let modified = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let extension = path
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase());
    let is_hidden = name.starts_with('.');
    Some(FileEntry {
        name,
        path: path.to_string_lossy().to_string(),
        is_dir,
        is_symlink,
        size,
        modified,
        extension,
        is_hidden,
    })
}

pub fn list_dir(path: &str, show_hidden: bool) -> Result<Vec<FileEntry>, String> {
    let p = PathBuf::from(path);
    let rd = fs::read_dir(&p).map_err(|e| format!("{}: {}", path, e))?;
    let mut out = Vec::new();
    for ent in rd.flatten() {
        if let Some(e) = to_entry(&ent.path()) {
            if !show_hidden && e.is_hidden {
                continue;
            }
            out.push(e);
        }
    }
    out.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(out)
}

pub fn home_dir() -> Result<String, String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "no home".to_string())
}

pub fn path_parent(path: &str) -> Option<String> {
    PathBuf::from(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
}

pub fn path_join(base: &str, child: &str) -> String {
    PathBuf::from(base).join(child).to_string_lossy().to_string()
}

pub fn make_dir(path: &str) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

pub fn rename_path(from: &str, to: &str) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| e.to_string())
}

fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_child = entry.path();
            let dst_child = dst.join(entry.file_name());
            copy_recursive(&src_child, &dst_child)?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
    }
    Ok(())
}

pub fn copy_paths(sources: &[String], dest_dir: &str) -> Result<(), String> {
    let dst_base = PathBuf::from(dest_dir);
    fs::create_dir_all(&dst_base).map_err(|e| e.to_string())?;
    for s in sources {
        let src = PathBuf::from(s);
        let name = src
            .file_name()
            .ok_or_else(|| format!("invalid source: {}", s))?;
        let dst = dst_base.join(name);
        copy_recursive(&src, &dst).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn move_paths(sources: &[String], dest_dir: &str) -> Result<(), String> {
    let dst_base = PathBuf::from(dest_dir);
    fs::create_dir_all(&dst_base).map_err(|e| e.to_string())?;
    for s in sources {
        let src = PathBuf::from(s);
        let name = src
            .file_name()
            .ok_or_else(|| format!("invalid source: {}", s))?;
        let dst = dst_base.join(name);
        if fs::rename(&src, &dst).is_err() {
            copy_recursive(&src, &dst).map_err(|e| e.to_string())?;
            if src.is_dir() {
                fs::remove_dir_all(&src).map_err(|e| e.to_string())?;
            } else {
                fs::remove_file(&src).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

pub fn delete_paths(paths: &[String], to_trash: bool) -> Result<(), String> {
    for p in paths {
        if to_trash {
            trash::delete(p).map_err(|e| e.to_string())?;
        } else {
            let pb = PathBuf::from(p);
            if pb.is_dir() {
                fs::remove_dir_all(&pb).map_err(|e| e.to_string())?;
            } else {
                fs::remove_file(&pb).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

pub fn list_drives() -> Vec<Drive> {
    let mut out = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            if std::path::Path::new(&drive).exists() {
                out.push(Drive {
                    name: format!("{}:", letter as char),
                    path: drive,
                });
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        out.push(Drive {
            name: "/".to_string(),
            path: "/".to_string(),
        });
        if let Some(h) = dirs::home_dir() {
            out.push(Drive {
                name: "Home".to_string(),
                path: h.to_string_lossy().to_string(),
            });
        }
        for extra in ["/Volumes", "/mnt", "/media"] {
            if std::path::Path::new(extra).exists() {
                if let Ok(rd) = std::fs::read_dir(extra) {
                    for e in rd.flatten() {
                        out.push(Drive {
                            name: e.file_name().to_string_lossy().to_string(),
                            path: e.path().to_string_lossy().to_string(),
                        });
                    }
                }
            }
        }
    }
    out
}
