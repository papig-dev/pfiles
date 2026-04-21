use serde::Serialize;
use std::fs;

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PreviewData {
    Text { content: String, truncated: bool },
    Image { mime: String, bytes: Vec<u8> },
    Binary { size: u64, mime: String },
}

const MAX_TEXT: u64 = 512 * 1024;
const MAX_IMAGE: u64 = 8 * 1024 * 1024;

pub fn preview_file(path: &str) -> Result<PreviewData, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        return Err("is a directory".into());
    }
    let size = meta.len();
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let top = mime.type_();
    if top == mime_guess::mime::IMAGE && size <= MAX_IMAGE {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        return Ok(PreviewData::Image {
            mime: mime.to_string(),
            bytes,
        });
    }
    if top == mime_guess::mime::TEXT
        || matches!(mime.subtype().as_str(), "json" | "xml" | "javascript" | "x-yaml" | "yaml")
        || is_likely_text(path, size)
    {
        let read_size = size.min(MAX_TEXT) as usize;
        let mut buf = vec![0u8; read_size];
        use std::io::Read;
        let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
        f.read_exact(&mut buf).map_err(|e| e.to_string())?;
        let content = String::from_utf8_lossy(&buf).to_string();
        return Ok(PreviewData::Text {
            content,
            truncated: size > MAX_TEXT,
        });
    }
    Ok(PreviewData::Binary {
        size,
        mime: mime.to_string(),
    })
}

fn is_likely_text(path: &str, size: u64) -> bool {
    if size > 8192 {
        return false;
    }
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let slice = &data[..data.len().min(2048)];
    !slice.contains(&0)
}
