//! Files the shell writes, and where. Every write is atomic: a temp file in
//! the same directory, fsync, rename, so a crash never leaves a half-written
//! save, index or setting behind.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Write in the same directory so rename atomically replaces the old file.
/// create_new avoids accidentally following an existing temp file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("{}: {e}", temp.display()))?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| format!("Cannot save {}: {e}", path.display()))
}

pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unset; use --data-dir".to_string())
}

pub fn default_data_dir() -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return Ok(home()?.join("Library/Application Support/Emulia"));
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path.join("emulia"));
    }
    Ok(home()?.join(".local/share/emulia"))
}

/// `~/Pictures/Emulia`, honouring `XDG_PICTURES_DIR` from user-dirs.dirs on Linux.
pub fn pictures_dir() -> Result<PathBuf, String> {
    let home = home()?;
    if !cfg!(target_os = "macos") {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        if let Ok(text) = fs::read_to_string(config.join("user-dirs.dirs")) {
            for line in text.lines() {
                if let Some(value) = line.trim().strip_prefix("XDG_PICTURES_DIR=") {
                    let value = value
                        .trim_matches('"')
                        .replace("$HOME", &home.to_string_lossy());
                    return Ok(PathBuf::from(value).join("Emulia"));
                }
            }
        }
    }
    Ok(home.join("Pictures/Emulia"))
}

pub fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// RGBA8, top-down.
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    write_atomic(path, &out)
}

/// Any PNG the shell wrote, back as RGBA8 top-down.
pub fn read_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        other => return Err(format!("unsupported PNG colour type {other:?}")),
    };
    Ok((info.width, info.height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_save_replaces_and_preserves_previous_file_on_collision() {
        let dir = temp_dir("files-atomic");
        let path = dir.join("game.sav");
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        let temp = path.with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temp, b"existing temporary file").unwrap();
        assert!(write_atomic(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read(&temp).unwrap(), b"existing temporary file");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn png_round_trips_rgba() {
        let dir = temp_dir("files-png");
        let path = dir.join("thumb.png");
        let pixels: Vec<u8> = (0..4 * 3 * 2).map(|i| (i * 7) as u8).collect();
        write_png(&path, 3, 2, &pixels).unwrap();
        let (w, h, back) = read_png(&path).unwrap();
        assert_eq!((w, h), (3, 2));
        assert_eq!(back, pixels);
        fs::remove_dir_all(dir).unwrap();
    }
}
