//! Otomatik güncelleme: GitHub'daki son sürümü denetler. Windows'ta exe'yi indirip
//! doğrular ve yerine koyar; yeni sürüm bir sonraki açılışta çalışır.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

const LATEST: &str = "https://api.github.com/repos/misutesu-desu/swift-explorer/releases/latest";
const ASSET: &str = "SwiftExplorer.exe";

/// Güncellemeyi kuran gizli süreç bu bayrakla yeni sürümü başlatır; yeni süreç
/// eskisinin çıkmasını bekleyip arka planda bekler.
pub const AFTER_UPDATE_ARG: &str = "--after-update";

pub enum Found {
    /// Yeni exe yerine kondu
    Installed(String),
    /// Kurulamıyor (exe yazılamayan bir yerde ya da Windows dışı); sürüm ve sayfası
    Available(String, String),
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

/// "v0.5.1" > "0.5.0"; sayı olmayan etiketler yeni sayılmaz
fn newer(tag: &str, current: &str) -> bool {
    let parse = |s: &str| -> Option<Vec<u64>> { s.trim_start_matches('v').split('.').map(|p| p.parse().ok()).collect() };
    match (parse(tag), parse(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Geliştirme derlemesi (target/ altında) kendini güncellemesin
fn is_dev_build(exe: &Path) -> bool {
    cfg!(debug_assertions) || exe.ancestors().any(|a| a.file_name().is_some_and(|n| n == "target"))
}

/// Scoop kurduğu klasöre bu iki dosyayı koyar; sürümü kendisi takip eder
fn scoop_install(exe: &Path) -> bool {
    exe.parent().is_some_and(|d| d.join("install.json").exists() && d.join("manifest.json").exists())
}

/// Flatpak ve Scoop kendi güncellemesini yapar
pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("FLATPAK_ID").is_none() && !std::env::current_exe().is_ok_and(|e| scoop_install(&e)))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(30)).build()
}

/// Son sürümü denetler, Windows'ta kurar. Yeni sürüm yoksa `None`.
pub fn check() -> Result<Option<Found>, String> {
    let ua = format!("SwiftExplorer/{}", env!("CARGO_PKG_VERSION"));
    let mut body = Vec::new();
    agent()
        .get(LATEST)
        .set("User-Agent", &ua)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?
        .into_reader()
        .take(4 << 20)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    let rel: Release = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if !newer(&rel.tag_name, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let version = rel.tag_name.trim_start_matches('v').to_string();
    let available = Ok(Some(Found::Available(version.clone(), rel.html_url.clone())));
    if !cfg!(windows) {
        return available;
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let asset = rel.assets.iter().find(|a| a.name == ASSET);
    let sha = asset.and_then(|a| a.digest.as_deref()?.strip_prefix("sha256:"));
    let (Some(asset), Some(sha)) = (asset, sha) else { return available };
    if is_dev_build(&exe) {
        return available;
    }
    match install(&exe, &asset.browser_download_url, sha, &ua) {
        Ok(()) => Ok(Some(Found::Installed(version))),
        // Program Files gibi yazılamayan bir klasör: kullanıcı kendisi indirir
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => available,
        Err(e) => Err(e.to_string()),
    }
}

fn with_ext(exe: &Path, ext: &str) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(ext);
    PathBuf::from(s)
}

/// Önceki güncellemeden kalan eski exe; onu çalıştıran süreç kapandıysa silinebilir
pub fn remove_old() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(with_ext(&exe, ".old"));
    }
}

fn install(exe: &Path, url: &str, sha: &str, ua: &str) -> std::io::Result<()> {
    let new = with_ext(exe, ".new");
    let old = with_ext(exe, ".old");
    let resp = agent().get(url).set("User-Agent", ua).call().map_err(std::io::Error::other)?;
    let mut file = std::fs::File::create(&new)?;
    let mut hasher = Sha256::new();
    let mut reader = resp.into_reader().take(256 << 20);
    let mut buf = vec![0u8; 64 << 10];
    let res = (|| {
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])?;
        }
        file.sync_all()?;
        drop(file);
        let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if !got.eq_ignore_ascii_case(sha) {
            return Err(std::io::Error::other("sha256 eşleşmedi"));
        }
        // Çalışan exe silinemez ama yeniden adlandırılabilir
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old)?;
        if let Err(e) = std::fs::rename(&new, exe) {
            let _ = std::fs::rename(&old, exe);
            return Err(e);
        }
        Ok(())
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&new);
    }
    res
}

/// Kurulan sürümü pencere göstermeden başlatır
pub fn launch_hidden() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg(AFTER_UPDATE_ARG).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::{install, newer, with_ext};

    /// Ağ gerektirir: cargo test -- --ignored
    #[test]
    #[ignore]
    fn install_swaps_the_exe() {
        let url = "https://github.com/misutesu-desu/swift-explorer/releases/download/v0.5.0/SwiftExplorer.exe";
        let sha = "c04b3d39eb220c5958bfb0754977e5dd0891206f0808889b244fb3b01ddca3ee";
        let dir = std::env::temp_dir().join(format!("swift-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("SwiftExplorer.exe");
        std::fs::write(&exe, b"old").unwrap();

        assert!(install(&exe, url, &"0".repeat(64), "test").is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        assert!(!with_ext(&exe, ".new").exists());

        install(&exe, url, sha, "test").unwrap();
        assert_eq!(std::fs::read(with_ext(&exe, ".old")).unwrap(), b"old");
        assert_eq!(std::fs::metadata(&exe).unwrap().len(), 7398912);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn version_order() {
        assert!(newer("v0.5.1", "0.5.0"));
        assert!(newer("v0.10.0", "0.9.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("v0.5.0", "0.5.0"));
        assert!(!newer("v0.4.9", "0.5.0"));
        assert!(!newer("nightly", "0.5.0"));
    }
}
