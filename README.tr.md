# Swift Explorer

[English](README.md)

Windows 11 Dosya Gezgini'nin görünümünde ve davranışında, Rust ile yazılmış bir dosya yöneticisi. Klasörler arka planda okunduğu için büyük klasörler pencereyi dondurmadan açılır; arama alt klasörlerde paralel yapılır.

![Türkçe arayüz, tarihe göre gruplanmış büyük simgeler](docs/turkish.png)

## İndirme

`SwiftExplorer.exe` dosyasını [Releases](../../releases) sayfasından indirin. Tek dosyadır, kurulum gerektirmez. Windows 11 için yapıldı; Windows 10'da da çalışması gerekir ama denenmedi.

## Neler var

- Explorer'la aynı düzen: başlık çubuğunda sekmeler, kırıntılı adres çubuğu, komut çubuğu, gezinti bölmesi, Giriş ve Bu bilgisayar sayfaları.
- Windows'un kendi ikonları, küçük resimleri ve tür adları kullanılır.
- Explorer'daki kısayolların hepsi aynı çalışır. F1 tam listeyi gösterir.
- Arama kutusuna yazdıkça bulunduğunuz klasör filtrelenir. Enter'a basınca tüm alt klasörlerde arar; `*` ve `?` joker karakterleri çalışır.
- Kopyala, kes, yapıştır ve sürükle-bırak Explorer ve masaüstüyle iki yönlü çalışır.
- Shift + sağ tık Windows'un tam sağ tık menüsünü açar; 7-Zip, WinRAR ve Gönder orada.
- Ctrl+Z yeniden adlandırma, taşıma, kopyalama ve geri dönüşüm kutusuna silme işlemlerini geri alır.
- Hedefte aynı adlı dosya varsa değiştirmek, atlamak ya da ikisini de saklamak arasında seçim sorar.
- Diskteki değişiklikler açık klasörde anında görünür.
- İndirilenler, Explorer'daki gibi değiştirme tarihine göre gruplanır (Bugün, Dün, Geçen hafta…). Diğer klasörler Sırala > Grupla'dan gruplanabilir.
- Türkçe ve İngilizce arayüz. Dil Windows'a göre seçilir, Görünüm > Dil'den değiştirilebilir.

Diğer ekran görüntüleri [İngilizce README](README.md)'de.

Tür sütunu ve Shift + sağ tık menüsü Windows'tan gelir; uygulama İngilizceye alınsa da bunlar Windows'un dilinde kalır.

## Derleme

Rust 1.82 ya da daha yenisi ve Windows gerekir.

```bash
cargo build --release
```

Çalıştırılabilir dosya `target/release/swift-explorer.exe` konumuna yazılır.

## Ayarlar

Ayarlar `%APPDATA%\SwiftExplorer\config.json` dosyasında tutulur. `SWIFT_EXPLORER_LANG` ortam değişkeni `en` ya da `tr` yapılırsa dil ayarının önüne geçer.

## Bilinen sınırlar

- Ayrıntılar görünümünde seçim dikdörtgeni yalnızca satırların altındaki boş alandan başlatılabilir.
- Windows çok uzun yollardaki (yaklaşık 260+ karakter) dosyaları geri dönüşüm kutusuna taşıyamaz; bu dosyalar doğrudan silinir ve Ctrl+Z geri getiremez.
- Ağ konumları Windows Dosya Gezgini'nde açılır.

## Lisans

[MIT](LICENSE)
