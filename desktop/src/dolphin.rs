//! Separately installed Dolphin owns gameplay and saves. Emulia owns only its
//! file reference and the process it starts; no shell command is constructed.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Disc {
    pub path: PathBuf,
    pub system: String,
    pub disc_id: String,
    pub sha256: String,
    #[serde(default)]
    pub nkit: bool,
}
impl Disc {
    pub fn inspect(path: &Path) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}. Relink the disc if it moved.", path.display()))?;
        let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Choose a disc image file".into());
        }
        let mut header = [0; 544];
        file.read_exact(&mut header)
            .map_err(|e| format!("Cannot read disc header: {e}"))?;
        let system = if header[28..32] == [0xc2, 0x33, 0x9f, 0x3d] {
            "gamecube"
        } else if header[24..28] == [0x5d, 0x1c, 0x9e, 0xa3] {
            "wii"
        } else {
            return Err("Choose a GameCube or Wii ISO/GCM image".into());
        };
        let mut hash = Sha256::new();
        hash.update(header);
        let mut buffer = [0; 65536];
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        Ok(Self {
            path,
            system: system.into(),
            disc_id: String::from_utf8_lossy(&header[..6]).into_owned(),
            sha256: format!("{:x}", hash.finalize()),
            nkit: &header[512..516] == b"NKIT",
        })
    }
    pub fn id(&self) -> String {
        format!("dolphin-{}", self.sha256)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Profile {
    pub executable: Option<PathBuf>,
}
impl Profile {
    pub fn load(dir: &Path) -> Result<Self, String> {
        match crate::files::read_optional(&dir.join("dolphin.json"))? {
            Some(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("Dolphin configuration: {e}"))
            }
            None => Ok(Self::default()),
        }
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        crate::files::write_atomic(
            &dir.join("dolphin.json"),
            &serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
    }
    pub fn resolve(&self) -> Result<PathBuf, String> {
        if let Some(path) = &self.executable {
            return executable(path);
        }
        let mut candidates = vec![PathBuf::from("/Applications/Dolphin.app")];
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join("Applications/Dolphin.app"));
        }
        if let Some(path) = std::env::var_os("PATH") {
            candidates.extend(std::env::split_paths(&path).map(|p| p.join("dolphin-emu")));
        }
        candidates
            .iter()
            .find_map(|p| executable(p).ok())
            .ok_or_else(|| "Install Dolphin and select its application in Settings.".into())
    }
}
pub fn executable(path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_dir() {
        path.join("Contents/MacOS/Dolphin")
    } else {
        path.to_path_buf()
    };
    if !path.is_file() {
        return Err(format!("Dolphin executable not found: {}", path.display()));
    }
    path.canonicalize().map_err(|e| e.to_string())
}

pub struct Running {
    child: Child,
    diagnostics: Arc<Mutex<VecDeque<u8>>>,
    pub title: String,
}
impl Running {
    pub fn launch(executable: &Path, disc: &Disc, title: &str) -> Result<Self, String> {
        // Verify the stored locator still addresses a readable regular image.
        let file = fs::File::open(&disc.path).map_err(|e| {
            format!(
                "{}: {e}. Relink the disc from its shelf menu.",
                disc.path.display()
            )
        })?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("The disc path is no longer a file".into());
        }
        let mut child = Command::new(executable)
            .arg("--batch")
            .arg("--exec")
            .arg(&disc.path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Cannot start Dolphin: {e}"))?;
        let diagnostics = Arc::new(Mutex::new(VecDeque::new()));
        let shared = diagnostics.clone();
        let mut stderr = child
            .stderr
            .take()
            .ok_or("Dolphin diagnostics pipe is missing")?;
        std::thread::spawn(move || {
            let mut chunk = [0; 2048];
            while let Ok(n) = stderr.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if let Ok(mut tail) = shared.lock() {
                    tail.extend(&chunk[..n]);
                    while tail.len() > 8192 {
                        tail.pop_front();
                    }
                }
            }
        });
        Ok(Self {
            child,
            diagnostics,
            title: title.into(),
        })
    }
    pub fn poll(&mut self) -> Result<Option<ExitStatus>, String> {
        self.child.try_wait().map_err(|e| e.to_string())
    }
    pub fn diagnostics(&self) -> String {
        self.diagnostics
            .lock()
            .map(|d| String::from_utf8_lossy(&d.iter().copied().collect::<Vec<_>>()).into_owned())
            .unwrap_or_default()
    }
}
// Dropping the launcher does not terminate Dolphin or its descendants.

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };

    fn directory(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("emulia-dolphin-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn disc_file(path: &Path, wii: bool) {
        let mut header = [0; 96];
        header[..6].copy_from_slice(b"TEST01");
        if wii {
            header[24..28].copy_from_slice(&[0x5d, 0x1c, 0x9e, 0xa3]);
        } else {
            header[28..32].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
        }
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&header).unwrap();
        file.set_len(17 * 1024 * 1024).unwrap();
    }
    #[test]
    fn disc_import_relink_and_identity_preserve_original_files() {
        let dir = directory("storage");
        let path = dir.join("Game 日本語 (USA); $literal.iso");
        disc_file(&path, false);
        let library = crate::library::Library::open(&dir.join("app-data")).unwrap();
        let game = crate::import(&library, &path).unwrap();
        assert!(game.is_external());
        assert_eq!(library.disc(&game.id).unwrap().system, "gamecube");
        assert!(!library.directory(&game.id).join("game.nes").exists());
        assert!(!library.directory(&game.id).join("game.iso").exists());
        assert_eq!(crate::import(&library, &path).unwrap(), game);
        let moved = dir.join("moved.iso");
        fs::rename(&path, &moved).unwrap();
        library.relink_disc(&game.id, &moved).unwrap();
        assert_eq!(
            library.disc(&game.id).unwrap().path,
            moved.canonicalize().unwrap()
        );
        let wrong = dir.join("wii.iso");
        disc_file(&wrong, true);
        assert_eq!(Disc::inspect(&wrong).unwrap().system, "wii");
        assert!(library.relink_disc(&game.id, &wrong).is_err());
        assert_eq!(
            library.disc(&game.id).unwrap().path,
            moved.canonicalize().unwrap()
        );
        assert_eq!(game.seconds, 0);
        library.forget(&game.id).unwrap();
        assert!(
            moved.exists(),
            "forgetting a shelf entry must never remove its external disc"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    fn script(path: &Path, text: &str) {
        fs::write(path, format!("#!/bin/sh\n{text}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn wait(run: &mut Running) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = run.poll().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "launcher child did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn launcher_passes_literal_paths_and_drains_bounded_errors() {
        let dir = directory("process");
        let path = dir.join("disc ; $(touch SHOULD_NOT_EXIST) 日本語.iso");
        disc_file(&path, false);
        let disc = Disc::inspect(&path).unwrap();
        let executable = dir.join("fake Dolphin");
        let output = dir.join("arguments");
        script(
            &executable,
            &format!("printf '%s\\n' \"$@\" > '{}'\nexit 0", output.display()),
        );
        let mut run = Running::launch(&executable, &disc, "Test").unwrap();
        assert!(wait(&mut run).success());
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            format!("--batch\n--exec\n{}\n", disc.path.display())
        );
        script(&executable, "head -c 1048576 /dev/zero >&2\nexit 7");
        let mut run = Running::launch(&executable, &disc, "Test").unwrap();
        assert_eq!(wait(&mut run).code(), Some(7));
        assert!(run.diagnostics().len() <= 8192);
        assert!(Running::launch(&dir.join("missing"), &disc, "Test").is_err());
        fs::remove_file(&path).unwrap();
        assert!(Running::launch(&executable, &disc, "Test").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn configured_bundle_resolves_and_bad_settings_are_not_replaced() {
        let dir = directory("profile");
        let bundle = dir.join("Dolphin.app");
        let binary = bundle.join("Contents/MacOS/Dolphin");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        script(&binary, "exit 0");
        Profile {
            executable: Some(bundle),
        }
        .save(&dir)
        .unwrap();
        assert_eq!(
            Profile::load(&dir).unwrap().resolve().unwrap(),
            binary.canonicalize().unwrap()
        );
        fs::write(dir.join("dolphin.json"), b"broken").unwrap();
        assert!(Profile::load(&dir).is_err());
        assert_eq!(fs::read(dir.join("dolphin.json")).unwrap(), b"broken");
        fs::remove_dir_all(dir).unwrap();
    }
}
