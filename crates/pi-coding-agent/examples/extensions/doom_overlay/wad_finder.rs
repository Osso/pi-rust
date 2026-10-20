//! Explicit native module directory replaces TS `import.meta.url`.
//! Search/download is opt-in; errors return None, as in the pinned helper.
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use flate2::read::MultiGzDecoder;
use pi_ai::request::{Fetch, HttpRequest};

const WAD_URL: &str = "https://www.gamers.org/pub/idgames/idstuff/doom/doom-1.8.wad.gz";

pub struct WadFinder {
    bundled: PathBuf,
    cwd: PathBuf,
    home: String,
}

impl WadFinder {
    pub fn new(module_dir: &Path, cwd: &Path, home: Option<&str>) -> Self {
        Self {
            bundled: module_dir.join("doom1.wad"),
            cwd: cwd.to_owned(),
            home: home.unwrap_or_default().to_owned(),
        }
    }

    pub fn find_wad_file(&self, custom_path: Option<&str>) -> Option<PathBuf> {
        if let Some(custom) = custom_path.filter(|path| !path.is_empty()) {
            let path = self.resolve(custom);
            return path.exists().then_some(path);
        }
        if self.bundled.exists() {
            return Some(self.bundled.clone());
        }
        ["./doom1.wad", "./DOOM1.WAD", "~/doom1.wad", "~/.doom/doom1.wad"]
            .into_iter()
            .map(|path| self.resolve(path))
            .find(|path| path.exists())
    }

    pub async fn ensure_wad_file(&self, fetch: &dyn Fetch) -> Option<PathBuf> {
        if let Some(existing) = self.find_wad_file(None) {
            return Some(existing);
        }
        // Source deliberately catches HTTP, stream, gzip, IWAD and write errors.
        self.download_wad(fetch).await.ok()?;
        Some(self.bundled.clone())
    }

    async fn download_wad(&self, fetch: &dyn Fetch) -> Result<(), String> {
        let response = fetch
            .fetch(
                HttpRequest {
                    method: "GET".into(),
                    url: WAD_URL.into(),
                    headers: Vec::new(),
                    body: None,
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        if !(200..300).contains(&response.status) {
            return Err(format!("HTTP {}", response.status));
        }
        let mut compressed = Vec::new();
        let mut body = response.body;
        while let Some(chunk) = body.next_chunk().await {
            compressed.extend(chunk.map_err(|error| error.to_string())?);
        }
        let mut wad = Vec::new();
        MultiGzDecoder::new(compressed.as_slice())
            .read_to_end(&mut wad)
            .map_err(|error| error.to_string())?;
        // Node Buffer.toString("ascii") clears the high bit, unlike UTF-8.
        if !wad.iter().take(4).map(|byte| byte & 0x7f).eq(b"IWAD".iter().copied()) {
            return Err("Downloaded file is not a valid IWAD".into());
        }
        std::fs::write(&self.bundled, wad).map_err(|error| error.to_string())
    }

    fn resolve(&self, input: &str) -> PathBuf {
        let expanded = match input.strip_prefix('~') {
            Some(rest) => format!("{}{rest}", self.home),
            None => input.to_owned(),
        };
        let joined = self.cwd.join(expanded);
        let mut resolved = PathBuf::new();
        for part in joined.components() {
            match part {
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::CurDir => {}
                other => resolved.push(other.as_os_str()),
            }
        }
        resolved
    }
}
