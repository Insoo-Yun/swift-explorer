//! Otomatik güncelleme: GitHub'daki son sürümü denetler. Windows'ta exe'yi indirip
//! doğrular ve yerine koyar; yeni sürüm bir sonraki açılışta çalışır.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Her sürümle yayınlanan bilgi dosyası. API'ye gitmediği için istek sınırına takılmaz.
const LATEST: &str = "https://github.com/misutesu-desu/swift-explorer/releases/latest/download/latest.json";

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
struct Manifest {
    version: String,
    page: String,
    windows: Option<Binary>,
}

#[derive(Deserialize)]
struct Binary {
    url: String,
    sha256: String,
    /// `signed_message` için ed25519 imzası, onaltılık
    sig: String,
}

/// CI'daki UPDATE_SIGNING_KEY'in açık anahtarı. GitHub hesabı ele geçirilse bile
/// bu anahtarla imzalanmamış exe kurulmaz.
const PUBLIC_KEY: [u8; 32] = [
    0x3c, 0x90, 0xcc, 0x71, 0x67, 0xc3, 0x62, 0xc6, 0x09, 0x2c, 0x35, 0x0a, 0xc1, 0xda, 0xa9, 0xcb,
    0x6d, 0x55, 0x4c, 0xa1, 0x47, 0x14, 0x77, 0x09, 0x12, 0x75, 0xfb, 0x22, 0xd6, 0x17, 0xd9, 0x38,
];

/// build.yml'deki imzalanan metinle aynı olmalı
fn signed_message(version: &str, sha256: &str) -> String {
    format!("swift-explorer {version} {sha256}")
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn signature_ok(key: &[u8], version: &str, sha256: &str, sig: &str) -> bool {
    use ring::signature::{UnparsedPublicKey, ED25519};
    let Some(sig) = from_hex(sig) else { return false };
    UnparsedPublicKey::new(&ED25519, key).verify(signed_message(version, sha256).as_bytes(), &sig).is_ok()
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
        .call()
        .map_err(|e| e.to_string())?
        .into_reader()
        .take(64 << 10)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    let m: Manifest = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if !newer(&m.version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let available = Ok(Some(Found::Available(m.version.clone(), m.page.clone())));
    if !cfg!(windows) {
        return available;
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let Some(bin) = &m.windows else { return available };
    if is_dev_build(&exe) {
        return available;
    }
    if !signature_ok(&PUBLIC_KEY, &m.version, &bin.sha256, &bin.sig) {
        return Err("güncelleme imzası geçersiz".into());
    }
    match install(&exe, &bin.url, &bin.sha256, &ua) {
        Ok(()) => Ok(Some(Found::Installed(m.version))),
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
    use super::{install, newer, signature_ok, signed_message, with_ext};

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
    fn signature_checks_version_and_hash() {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let key = pair.public_key().as_ref();
        let sha = "ab".repeat(32);
        let sig: String = pair.sign(signed_message("0.7.0", &sha).as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect();

        assert!(signature_ok(key, "0.7.0", &sha, &sig));
        assert!(!signature_ok(key, "0.8.0", &sha, &sig));
        assert!(!signature_ok(key, "0.7.0", &"cd".repeat(32), &sig));
        assert!(!signature_ok(key, "0.7.0", &sha, "zz"));
        assert!(!signature_ok(&[0u8; 32], "0.7.0", &sha, &sig));
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
