fn main() {
    // Windows'ta exe dosyasına uygulama ikonunu göm
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().expect("ikon kaynağı derlenemedi");
    }
}
