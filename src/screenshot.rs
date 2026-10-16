//! README görüntülerini yeniden üretmek için: `--features screenshot` ile derlenip
//! `SWIFT_EXPLORER_SCREENSHOT=çıktı.bmp` verilince pencere birkaç saniye sonra kendi
//! görüntüsünü BMP olarak kaydeder ve kapanır. Pencere gizli bir masaüstünde olsa da çalışır.
//!
//! `SWIFT_EXPLORER_SCREENSHOT_SIZE=1280x800` pencere boyutunu,
//! `SWIFT_EXPLORER_SCREENSHOT_DELAY=4` beklenecek saniyeyi belirler.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Shot {
    path: PathBuf,
    at: Instant,
    size: Option<egui::Vec2>,
    requested: bool,
}

static SHOT: Mutex<Option<Shot>> = Mutex::new(None);

pub fn init() {
    let Some(path) = std::env::var_os("SWIFT_EXPLORER_SCREENSHOT") else { return };
    let delay = std::env::var("SWIFT_EXPLORER_SCREENSHOT_DELAY").ok().and_then(|s| s.parse().ok()).unwrap_or(4.0);
    let size = std::env::var("SWIFT_EXPLORER_SCREENSHOT_SIZE").ok().and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some(egui::vec2(w.parse().ok()?, h.parse().ok()?))
    });
    *SHOT.lock().unwrap() = Some(Shot { path: path.into(), at: Instant::now() + Duration::from_secs_f32(delay), size, requested: false });
}

/// Her karede çağrılır
pub fn tick(ctx: &egui::Context) {
    let mut guard = SHOT.lock().unwrap();
    let Some(shot) = guard.as_mut() else { return };
    if let Some(size) = shot.size.take() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
    }
    let image = ctx.input(|i| {
        i.raw.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(image) = image {
        if let Err(e) = save_bmp(&shot.path, &image) {
            eprintln!("screenshot: {e}");
        }
        *guard = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }
    if !shot.requested && Instant::now() >= shot.at {
        shot.requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
    }
    ctx.request_repaint_after(Duration::from_millis(100));
}

/// 32 bit, üstten alta BMP
fn save_bmp(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let [w, h] = img.size;
    let data_len = (w * h * 4) as u32;
    let mut out = Vec::with_capacity(54 + data_len as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(54 + data_len).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for p in &img.pixels {
        let [r, g, b, a] = p.to_srgba_unmultiplied();
        out.extend_from_slice(&[b, g, r, a]);
    }
    std::fs::write(path, out)
}
