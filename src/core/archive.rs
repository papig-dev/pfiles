use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ArchiveEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
}

fn archive_kind(path: &str) -> ArchiveKind {
    let lower = path.to_lowercase();
    if lower.ends_with(".zip") || lower.ends_with(".jar") {
        ArchiveKind::Zip
    } else if lower.ends_with(".7z") {
        ArchiveKind::SevenZ
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        ArchiveKind::TarGz
    } else if lower.ends_with(".tar") {
        ArchiveKind::Tar
    } else if lower.ends_with(".gz") {
        ArchiveKind::Gz
    } else {
        ArchiveKind::Unknown
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    Gz,
    Unknown,
}

pub fn is_archive(path: &str) -> bool {
    archive_kind(path) != ArchiveKind::Unknown
}

pub fn list_archive(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    match archive_kind(path) {
        ArchiveKind::Zip => list_zip(path).map_err(|e| e.to_string()),
        ArchiveKind::SevenZ => list_7z(path).map_err(|e| e.to_string()),
        ArchiveKind::Tar => list_tar(path, false).map_err(|e| e.to_string()),
        ArchiveKind::TarGz => list_tar(path, true).map_err(|e| e.to_string()),
        ArchiveKind::Gz => Ok(vec![ArchiveEntry {
            name: Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "data".into()),
            is_dir: false,
            size: 0,
            modified: None,
        }]),
        ArchiveKind::Unknown => Err("unsupported archive".into()),
    }
}

fn list_zip(path: &str) -> io::Result<Vec<ArchiveEntry>> {
    let f = File::open(path)?;
    let mut z = zip::ZipArchive::new(f).map_err(io_err)?;
    let mut out = Vec::with_capacity(z.len());
    for i in 0..z.len() {
        let file = z.by_index(i).map_err(io_err)?;
        let name = file.name().to_string();
        let is_dir = file.is_dir() || name.ends_with('/');
        out.push(ArchiveEntry {
            name,
            is_dir,
            size: file.size(),
            modified: file
                .last_modified()
                .and_then(|t| t.to_time().ok())
                .map(|t| t.unix_timestamp()),
        });
    }
    Ok(out)
}

fn list_7z(path: &str) -> io::Result<Vec<ArchiveEntry>> {
    let mut out = Vec::new();
    let file = File::open(path)?;
    let mut reader =
        sevenz_rust2::SevenZReader::new(file, sevenz_rust2::Password::empty()).map_err(io_err)?;
    reader
        .for_each_entries(|entry, _reader| {
            out.push(ArchiveEntry {
                name: entry.name.clone(),
                is_dir: entry.is_directory(),
                size: entry.size(),
                modified: None,
            });
            Ok(true)
        })
        .map_err(io_err)?;
    Ok(out)
}

fn list_tar(path: &str, gz: bool) -> io::Result<Vec<ArchiveEntry>> {
    let f = File::open(path)?;
    let mut out = Vec::new();
    if gz {
        let dec = flate2::read::GzDecoder::new(f);
        let mut archive = tar::Archive::new(dec);
        for entry in archive.entries()? {
            let entry = entry?;
            let path = entry.path()?.to_string_lossy().to_string();
            let is_dir = entry.header().entry_type().is_dir();
            out.push(ArchiveEntry {
                name: path,
                is_dir,
                size: entry.header().size().unwrap_or(0),
                modified: entry.header().mtime().ok().map(|v| v as i64),
            });
        }
    } else {
        let mut archive = tar::Archive::new(f);
        for entry in archive.entries()? {
            let entry = entry?;
            let path = entry.path()?.to_string_lossy().to_string();
            let is_dir = entry.header().entry_type().is_dir();
            out.push(ArchiveEntry {
                name: path,
                is_dir,
                size: entry.header().size().unwrap_or(0),
                modified: entry.header().mtime().ok().map(|v| v as i64),
            });
        }
    }
    Ok(out)
}

pub fn extract_archive(archive_path: &str, dest_dir: &str) -> Result<(), String> {
    let dest = PathBuf::from(dest_dir);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    match archive_kind(archive_path) {
        ArchiveKind::Zip => extract_zip(archive_path, &dest).map_err(|e| e.to_string()),
        ArchiveKind::SevenZ => {
            sevenz_rust2::decompress_file(archive_path, &dest).map_err(|e| e.to_string())
        }
        ArchiveKind::Tar => extract_tar(archive_path, &dest, false).map_err(|e| e.to_string()),
        ArchiveKind::TarGz => extract_tar(archive_path, &dest, true).map_err(|e| e.to_string()),
        ArchiveKind::Gz => extract_gz(archive_path, &dest).map_err(|e| e.to_string()),
        ArchiveKind::Unknown => Err("unsupported archive".into()),
    }
}

fn extract_zip(path: &str, dest: &Path) -> io::Result<()> {
    let f = File::open(path)?;
    let mut z = zip::ZipArchive::new(f).map_err(io_err)?;
    for i in 0..z.len() {
        let mut file = z.by_index(i).map_err(io_err)?;
        let outpath = match file.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if file.is_dir() || file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                fs::create_dir_all(p)?;
            }
            let mut out = File::create(&outpath)?;
            io::copy(&mut file, &mut out)?;
        }
    }
    Ok(())
}

fn extract_tar(path: &str, dest: &Path, gz: bool) -> io::Result<()> {
    let f = File::open(path)?;
    if gz {
        let dec = flate2::read::GzDecoder::new(f);
        let mut archive = tar::Archive::new(dec);
        archive.unpack(dest)
    } else {
        let mut archive = tar::Archive::new(f);
        archive.unpack(dest)
    }
}

fn extract_gz(path: &str, dest: &Path) -> io::Result<()> {
    let f = File::open(path)?;
    let name = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "data".into());
    let mut dec = flate2::read::GzDecoder::new(f);
    let outpath = dest.join(name);
    let mut out = File::create(&outpath)?;
    io::copy(&mut dec, &mut out)?;
    Ok(())
}

pub fn create_zip(sources: &[String], out_path: &str) -> Result<(), String> {
    let out = File::create(out_path).map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new(out);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for s in sources {
        let src = PathBuf::from(s);
        let base_name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "entry".into());
        if src.is_dir() {
            add_dir_to_zip(&src, &base_name, &mut zw, opts).map_err(|e| e.to_string())?;
        } else {
            add_file_to_zip(&src, &base_name, &mut zw, opts).map_err(|e| e.to_string())?;
        }
    }
    zw.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn add_file_to_zip<W: Write + Seek>(
    src: &Path,
    name: &str,
    zw: &mut zip::ZipWriter<W>,
    opts: zip::write::SimpleFileOptions,
) -> io::Result<()> {
    zw.start_file(name, opts).map_err(io_err)?;
    let mut f = File::open(src)?;
    io::copy(&mut f, zw)?;
    Ok(())
}

fn add_dir_to_zip<W: Write + Seek>(
    src: &Path,
    prefix: &str,
    zw: &mut zip::ZipWriter<W>,
    opts: zip::write::SimpleFileOptions,
) -> io::Result<()> {
    let dir_name = format!("{}/", prefix);
    zw.add_directory(&dir_name, opts).map_err(io_err)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
        if p.is_dir() {
            add_dir_to_zip(&p, &name, zw, opts)?;
        } else {
            add_file_to_zip(&p, &name, zw, opts)?;
        }
    }
    Ok(())
}

fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}
