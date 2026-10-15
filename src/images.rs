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

/// Arka plan iş parçacıklarında ikon/küçük resim üretip doku olarak önbelleğe alır.
/// İstekler LIFO işlenir: hızlı kaydırmada ekrandaki öğeler önce gelir.
pub struct ImageCache {
    textures: HashMap<String, Img>,
    queue: Arc<(Mutex<Vec<Req>>, Condvar)>,
    rx: Receiver<(String, Option<egui::ColorImage>)>,
    thumbs: usize,
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
        ImageCache { textures: HashMap::new(), queue, rx, thumbs: 0 }
    }

    pub fn get(&mut self, key: &str, path: &Path, size: i32, icon_only: bool) -> Img {
        if let Some(t) = self.textures.get(key) {
            return t.clone();
        }
        self.textures.insert(key.to_string(), Img::Pending);
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
        while let Ok((key, img)) = self.rx.try_recv() {
            if !key.starts_with('i') {
                self.thumbs += 1;
            }
            let tex = match img {
                Some(i) => Img::Ready(ctx.load_texture(&key, i, egui::TextureOptions::LINEAR)),
                None => Img::Failed,
            };
            self.textures.insert(key, tex);
        }
        // Bellek sınırı: çok fazla küçük resim birikirse onları bırak (ikonlar kalır)
        if self.thumbs > 2500 {
            self.textures.retain(|k, _| k.starts_with('i'));
            self.thumbs = 0;
        }
    }
}
