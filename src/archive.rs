//! Zip arşivini açma ve oluşturma; ikisi de arka planda, ilerleme kartıyla.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::thread;

use crate::fsutil::{unique_destination, Job, JobKind};

pub fn is_zip(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Arşivi yanındaki "ad" klasörüne açar; o ad doluysa "ad - Kopya"
pub fn spawn_extract(zip: PathBuf, ctx: egui::Context) -> Arc<Job> {
    let name = zip.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let job = Job::new(crate::i18n::tf("Çıkartılıyor: {}", &[&name]), JobKind::Archive);
    let j = job.clone();
    thread::spawn(move || {
        let parent = zip.parent().unwrap_or(Path::new("")).to_path_buf();
        let dest = unique_destination(&parent, &name, true);
        match extract(&zip, &dest, &j) {
            Ok(()) => j.pairs.lock().unwrap().push((zip.clone(), dest)),
            Err(e) => {
                // Yarım kalan klasör bırakılmaz
                let _ = fs::remove_dir_all(&dest);
                if e.kind() != io::ErrorKind::Interrupted {
                    j.err(format!("{}: {e}", zip.display()));
                }
            }
        }
        j.finish();
        ctx.request_repaint();
    });
    job
}

fn extract(zip: &Path, dest: &Path, j: &Job) -> io::Result<()> {
    let mut ar = zip::ZipArchive::new(fs::File::open(zip)?).map_err(io::Error::other)?;
    j.total.store(ar.len() as u64, Relaxed);
    fs::create_dir_all(dest)?;
    for i in 0..ar.len() {
        if j.cancel.load(Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, crate::i18n::t("iptal edildi")));
        }
        let mut f = ar.by_index(i).map_err(io::Error::other)?;
        // "../" ile arşivin dışına yazmaya çalışan girdiler atlanır
        let Some(rel) = f.enclosed_name() else { continue };
        let out = dest.join(&rel);
        *j.current.lock().unwrap() = rel.to_string_lossy().into_owned();
        if f.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() {
                fs::create_dir_all(p)?;
            }
            io::copy(&mut f, &mut fs::File::create(&out)?)?;
        }
        j.done.fetch_add(1, Relaxed);
    }
    Ok(())
}

/// Öğeleri ilkinin adını taşıyan bir zip dosyasına sıkıştırır
pub fn spawn_compress(paths: Vec<PathBuf>, ctx: egui::Context) -> Arc<Job> {
    let first = &paths[0];
    let stem = if first.is_dir() { first.file_name() } else { first.file_stem() }.unwrap_or_default().to_string_lossy().into_owned();
    let job = Job::new(crate::i18n::tf("Sıkıştırılıyor: {}", &[&format!("{stem}.zip")]), JobKind::Archive);
    let j = job.clone();
    thread::spawn(move || {
        let parent = paths[0].parent().unwrap_or(Path::new("")).to_path_buf();
        let dest = unique_destination(&parent, &format!("{stem}.zip"), false);
        match compress(&paths, &parent, &dest, &j) {
            Ok(()) => j.pairs.lock().unwrap().push((dest.clone(), dest)),
            Err(e) => {
                let _ = fs::remove_file(&dest);
                if e.kind() != io::ErrorKind::Interrupted {
                    j.err(format!("{}: {e}", dest.display()));
                }
            }
        }
        j.finish();
        ctx.request_repaint();
    });
    job
}

fn walk(p: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    out.push(p.to_path_buf());
    if p.is_dir() && !fs::symlink_metadata(p)?.file_type().is_symlink() {
        for e in fs::read_dir(p)? {
            walk(&e?.path(), out)?;
        }
    }
    Ok(())
}

fn compress(paths: &[PathBuf], base: &Path, dest: &Path, j: &Job) -> io::Result<()> {
    let mut all = Vec::new();
    for p in paths {
        walk(p, &mut all)?;
    }
    j.total.store(all.len() as u64, Relaxed);
    let mut w = zip::ZipWriter::new(fs::File::create(dest)?);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    for p in &all {
        if j.cancel.load(Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, crate::i18n::t("iptal edildi")));
        }
        // Zip içinde ayraç her sistemde "/"
        let rel = p.strip_prefix(base).unwrap_or(p).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        *j.current.lock().unwrap() = rel.clone();
        if p.is_dir() {
            w.add_directory(rel, opts).map_err(io::Error::other)?;
        } else {
            w.start_file(rel, opts).map_err(io::Error::other)?;
            io::copy(&mut fs::File::open(p)?, &mut w)?;
        }
        j.done.fetch_add(1, Relaxed);
    }
    w.finish().map_err(io::Error::other)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_then_extract_round_trip() {
        let root = std::env::temp_dir().join(format!("swift-explorer-zip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/alt klasör")).unwrap();
        fs::write(root.join("src/a.txt"), "merhaba").unwrap();
        fs::write(root.join("src/alt klasör/b.bin"), [0u8, 1, 2, 255]).unwrap();
        let job = Job::new(String::new(), JobKind::Archive);
        let zip = root.join("src.zip");
        compress(&[root.join("src")], &root, &zip, &job).unwrap();
        assert_eq!(job.done.load(Relaxed), 4);
        let out = root.join("out");
        extract(&zip, &out, &job).unwrap();
        assert_eq!(fs::read_to_string(out.join("src/a.txt")).unwrap(), "merhaba");
        assert_eq!(fs::read(out.join("src/alt klasör/b.bin")).unwrap(), [0u8, 1, 2, 255]);
        job.finish();
        fs::remove_dir_all(&root).unwrap();
    }
}
