//! İkon ve küçük resimleri arka planda üretip doku olarak önbelleğe alır.
//! Görüntülerin kendisi platform modülünden (`shell::shell_image`) gelir.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
pub enum Img {
    Pending,
    Ready(egui::TextureHandle),
    Failed,
}

struct Req {
    key: String,
    path: PathBuf,
    size: i32,
    icon_only: bool,
}

/// Önbellekteki görüntülerin toplam piksel belleği (GPU/paylaşımlı bellek) üst sınırı.
/// Aşılınca en uzun süredir çizilmeyenler bırakılır; ekrandakiler hiç atılmaz.
const BUDGET_BYTES: usize = 160 << 20;

struct Slot {
    img: Img,
    /// En son istendiği kare
    used: u64,
}

/// Arka plan iş parçacıklarında ikon/küçük resim üretip doku olarak önbelleğe alır.
/// İstekler LIFO işlenir: hızlı kaydırmada ekrandaki öğeler önce gelir.
/// Piksel olarak aynı görüntüler (ör. binlerce standart klasör ikonu) tek dokuyu paylaşır.
pub struct ImageCache {
    textures: HashMap<String, Slot>,
    /// İçerik özeti → doku ve bayt boyutu
    unique: HashMap<u64, (egui::TextureHandle, usize)>,
    unique_bytes: usize,
    frame: u64,
    queue: Arc<(Mutex<Vec<Req>>, Condvar)>,
    rx: Receiver<(String, Option<egui::ColorImage>)>,
}

impl ImageCache {
    pub fn new(ctx: &egui::Context) -> ImageCache {
        let queue: Arc<(Mutex<Vec<Req>>, Condvar)> = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
        let (tx, rx): (Sender<(String, Option<egui::ColorImage>)>, _) = channel();
        for _ in 0..4 {
            let q = queue.clone();
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::shell::init_worker_thread();
                loop {
                    let req = {
                        let (m, cv) = &*q;
                        let mut g = m.lock().unwrap();
                        loop {
                            if let Some(r) = g.pop() {
                                break r;
                            }
                            g = cv.wait(g).unwrap();
                        }
                    };
                    // Shell ara sıra (özellikle aynı anda çok istekte) başarısız olur; birkaç kez dene
                    let mut img = None;
                    for attempt in 0..3 {
                        img = crate::shell::shell_image(&req.path, req.size, req.icon_only);
                        if img.is_some() {
                            break;
                        }
                        if !req.icon_only && attempt == 1 {
                            // Küçük resim yoksa ikona düş
                            img = crate::shell::shell_image(&req.path, req.size, true);
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(30));
                    }
                    if tx.send((req.key, img)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            });
        }
        ImageCache { textures: HashMap::new(), unique: HashMap::new(), unique_bytes: 0, frame: 0, queue, rx }
    }

    pub fn get(&mut self, key: &str, path: &Path, size: i32, icon_only: bool) -> Img {
        if let Some(t) = self.textures.get_mut(key) {
            t.used = self.frame;
            return t.img.clone();
        }
        self.textures.insert(key.to_string(), Slot { img: Img::Pending, used: self.frame });
        let (m, cv) = &*self.queue;
        m.lock().unwrap().push(Req { key: key.to_string(), path: path.to_path_buf(), size, icon_only });
        cv.notify_one();
        Img::Pending
    }

    /// Bekleyen (henüz işlenmemiş) istekleri atar; klasör değişince eski istekler beklemesin.
    pub fn drop_pending(&mut self) {
        let mut q = self.queue.0.lock().unwrap();
        for r in q.drain(..) {
            self.textures.remove(&r.key);
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        while let Ok((key, img)) = self.rx.try_recv() {
            let img = match img {
                Some(i) => Img::Ready(self.texture_for(ctx, &key, i)),
                None => Img::Failed,
            };
            self.textures.insert(key, Slot { img, used: self.frame });
        }
        if self.unique_bytes > BUDGET_BYTES {
            self.evict();
        }
    }

    /// Aynı piksellere sahip bir doku varsa onu paylaşır, yoksa yükler
    fn texture_for(&mut self, ctx: &egui::Context, key: &str, img: egui::ColorImage) -> egui::TextureHandle {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut h = egui::ahash::RandomState::with_seeds(1, 2, 3, 4).build_hasher();
        img.size.hash(&mut h);
        img.as_raw().hash(&mut h);
        let digest = h.finish();
        if let Some((tex, _)) = self.unique.get(&digest) {
            return tex.clone();
        }
        let bytes = img.pixels.len() * 4;
        let tex = ctx.load_texture(key, img, egui::TextureOptions::LINEAR);
        self.unique.insert(digest, (tex.clone(), bytes));
        self.unique_bytes += bytes;
        tex
    }

    /// En uzun süredir çizilmeyenleri bütçenin %75'ine inene dek bırakır
    fn evict(&mut self) {
        let mut order: Vec<(u64, String)> =
            self.textures.iter().filter(|(_, s)| s.used < self.frame.saturating_sub(1) && !matches!(s.img, Img::Pending)).map(|(k, s)| (s.used, k.clone())).collect();
        order.sort_unstable();
        let target = BUDGET_BYTES / 4 * 3;
        for chunk in order.chunks(64) {
            for (_, k) in chunk {
                self.textures.remove(k);
            }
            self.prune_unique();
            if self.unique_bytes <= target {
                break;
            }
        }
    }

    /// Artık hiçbir anahtarın göstermediği dokuları bırakır
    fn prune_unique(&mut self) {
        let live: std::collections::HashSet<egui::TextureId> =
            self.textures.values().filter_map(|s| if let Img::Ready(t) = &s.img { Some(t.id()) } else { None }).collect();
        let mut freed = 0;
        self.unique.retain(|_, (t, b)| {
            let keep = live.contains(&t.id());
            if !keep {
                freed += *b;
            }
            keep
        });
        self.unique_bytes -= freed;
    }
}
