//! İnce Win32 sarmalayıcıları: sürücüler, disk alanı, özellikler penceresi, pano.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};

const CREATE_NEW_CONSOLE: u32 = 0x10;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

#[derive(Clone)]
pub struct DriveInfo {
    pub root: PathBuf,
    pub label: String,
    pub free: u64,
    pub total: u64,
}

pub fn drives() -> Vec<DriveInfo> {
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = PathBuf::from(format!("{letter}:\\"));
        let w = wide(root.as_os_str());
        let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
        let kind_name = match kind {
            2 => crate::i18n::t("Çıkarılabilir disk"),
            3 => crate::i18n::t("Yerel Disk"),
            4 => crate::i18n::t("Ağ sürücüsü"),
            5 => "CD/DVD",
            6 => "RAM disk",
            _ => crate::i18n::t("Sürücü"),
        };
        let mut name_buf = [0u16; 261];
        let ok = unsafe {
            GetVolumeInformationW(
                w.as_ptr(),
                name_buf.as_mut_ptr(),
                name_buf.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        let vol = if ok != 0 {
            let len = name_buf.iter().position(|&c| c == 0).unwrap_or(0);
            String::from_utf16_lossy(&name_buf[..len])
        } else {
            String::new()
        };
        let label = if vol.is_empty() {
            format!("{kind_name} ({letter}:)")
        } else {
            format!("{vol} ({letter}:)")
        };
        let (free, total) = disk_space(&root).unwrap_or((0, 0));
        out.push(DriveInfo { root, label, free, total });
    }
    out
}

/// (boş, toplam) bayt
pub fn disk_space(path: &Path) -> Option<(u64, u64)> {
    let w = wide(path.as_os_str());
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some((avail, total))
}

/// Windows'un kendi "Özellikler" penceresini açar.
pub fn show_properties(path: &Path) {
    let file = wide(path.as_os_str());
    let verb = wide(OsStr::new("properties"));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = 0x0000_000C; // SEE_MASK_INVOKEIDLIST
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.nShow = 5; // SW_SHOW
        ShellExecuteExW(&mut info);
    }
}

pub fn open_with(path: &Path) {
    let _ = Command::new("rundll32.exe")
        .raw_arg(format!("shell32.dll,OpenAs_RunDLL {}", path.display()))
        .spawn();
}

pub fn show_in_explorer(path: &Path) {
    let _ = Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn();
}

pub fn open_terminal(dir: &Path) {
    let wt = Command::new("wt.exe")
        .arg("-d")
        .arg(dir)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    if wt.is_err() {
        let _ = Command::new("powershell.exe")
            .current_dir(dir)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn();
    }
}

/// Adres çubuğuna yazılınca o klasörde açılan terminaller (Explorer'daki gibi);
/// "terminal" Windows Terminal'i, yoksa PowerShell'i açar
const TERMINALS: [&str; 5] = ["terminal", "cmd", "powershell", "pwsh", "wt"];

pub fn is_terminal(name: &str) -> bool {
    let name = name.to_lowercase();
    TERMINALS.contains(&name.strip_suffix(".exe").unwrap_or(&name))
}

/// `name` terminalini `dir` klasöründe, varsa `args` ile başlatır
pub fn run_terminal(name: &str, args: &str, dir: &Path) -> bool {
    if name.eq_ignore_ascii_case("terminal") {
        open_terminal(dir);
        return true;
    }
    let mut cmd = Command::new(name);
    if name.to_lowercase().starts_with("wt") {
        cmd.arg("-d").arg(dir).creation_flags(CREATE_NO_WINDOW);
    } else {
        cmd.current_dir(dir).creation_flags(CREATE_NEW_CONSOLE);
    }
    if !args.is_empty() {
        cmd.raw_arg(args);
    }
    cmd.spawn().is_ok()
}

/// egui, panoda metin yoksa Ctrl+V için hiç olay üretmiyor; bu yüzden tuşu doğrudan soruyoruz.
pub fn ctrl_v_down() -> bool {
    unsafe {
        let ctrl = GetAsyncKeyState(0x11) as u16 & 0x8000 != 0;
        let v = GetAsyncKeyState(0x56) as u16 & 0x8000 != 0;
        ctrl && v
    }
}

// ---------- Pano (CF_HDROP) — Explorer ile karşılıklı kopyala/yapıştır ----------

const DROPEFFECT_COPY: u32 = 1;
const DROPEFFECT_MOVE: u32 = 2;

pub fn clipboard_set_files(paths: &[PathBuf], cut: bool) -> bool {
    use clipboard_win::{options::NoClear, raw, Clipboard};
    let Ok(_clip) = Clipboard::new_attempts(10) else { return false };
    if raw::empty().is_err() {
        return false;
    }
    let list: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    if raw::set_file_list_with(&list, NoClear).is_err() {
        return false;
    }
    if let Some(fmt) = raw::register_format("Preferred DropEffect") {
        let effect = if cut { DROPEFFECT_MOVE } else { DROPEFFECT_COPY };
        let _ = raw::set_without_clear(fmt.get(), &effect.to_le_bytes());
    }
    true
}

/// (dosyalar, kesme_mi)
pub fn clipboard_get_files() -> Option<(Vec<PathBuf>, bool)> {
    use clipboard_win::{raw, Clipboard};
    let _clip = Clipboard::new_attempts(10).ok()?;
    let mut files = Vec::new();
    raw::get_file_list_path(&mut files).ok()?;
    if files.is_empty() {
        return None;
    }
    let mut cut = false;
    if let Some(fmt) = raw::register_format("Preferred DropEffect") {
        let mut buf = Vec::new();
        if raw::get_vec(fmt.get(), &mut buf).is_ok() && buf.len() >= 4 {
            let v = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            cut = v & DROPEFFECT_MOVE != 0 && v & DROPEFFECT_COPY == 0;
        }
    }
    Some((files, cut))
}

// ---------- Arka planda bekleme: Explorer gibi anında açılmak için ----------
//
// Pencere ve OpenGL sürücüsünün açılışı bu uygulamanın kendi açılışından katbekat uzun
// sürüyor; hibrit grafikli dizüstülerde bir saniyeyi geçiyor. Bu yüzden pencere kapanınca
// süreç gizlenip bekler, sonraki açılış ona devredilir ve pencere hemen görünür.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::{AtomicIsize, Ordering::SeqCst};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, WaitNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, IsIconic, SetForegroundWindow, ShowWindow, ASFW_ANY, SW_HIDE, SW_RESTORE, SW_SHOW};

/// Gizlenip bekleyen pencere; 0 ise pencere açık ya da bekleme kapalı
static PARKED: AtomicIsize = AtomicIsize::new(0);
/// Devralınan açılış isteği: açılacak klasör (yoksa son sekmeler)
static REOPEN: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

/// Aynı oturumdaki aynı kullanıcıya özel boru adı
fn pipe_name() -> Vec<u16> {
    let mut session = 0;
    unsafe { ProcessIdToSessionId(std::process::id(), &mut session) };
    let user = std::env::var("USERNAME").unwrap_or_default();
    wide(OsStr::new(&format!(r"\\.\pipe\SwiftExplorer-{session}-{user}")))
}

/// Gizlenip bekleyen bir süreç varsa açılışı ona devreder. `true` dönerse pencere
/// o süreçte açıldı, bu süreç çıkabilir.
pub fn hand_off(start: Option<&Path>) -> bool {
    let name = pipe_name();
    let path = OsString::from_wide(&name[..name.len() - 1]);
    let mut pipe = None;
    for _ in 0..3 {
        match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
            Ok(f) => {
                pipe = Some(f);
                break;
            }
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => unsafe {
                WaitNamedPipeW(name.as_ptr(), 200);
            },
            Err(_) => return false,
        }
    }
    let Some(mut pipe) = pipe else { return false };
    let dir: Vec<u16> = start.and_then(|p| std::path::absolute(p).ok()).map(|p| p.as_os_str().encode_wide().collect()).unwrap_or_default();
    let mut msg = ((dir.len() * 2) as u32).to_le_bytes().to_vec();
    msg.extend(dir.iter().flat_map(|c| c.to_le_bytes()));
    // Kullanıcının başlattığı süreç öne çıkma hakkına sahip; bekleyen sürece devreder
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    let mut ok = [0u8];
    pipe.write_all(&msg).and_then(|_| pipe.read_exact(&mut ok)).is_ok() && ok[0] == 1
}

fn create_pipe(name: &[u16], first: bool) -> HANDLE {
    let mode = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
    unsafe { CreateNamedPipeW(name.as_ptr(), mode, PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, PIPE_UNLIMITED_INSTANCES, 512, 512, 0, std::ptr::null()) }
}

/// Sonraki açılışları dinler. Boruyu başka bir süreç tutuyorsa `false` döner;
/// o zaman bu pencere kapanınca süreç de kapanır.
pub fn serve(ctx: egui::Context) -> bool {
    let name = pipe_name();
    let first = create_pipe(&name, true);
    if first == INVALID_HANDLE_VALUE {
        return false;
    }
    // HANDLE ham işaretçi, iş parçacığına sayı olarak geçer
    let first = first as isize;
    std::thread::spawn(move || {
        let mut h = first as HANDLE;
        loop {
            let connected = unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) } != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            // Bu istek işlenirken gelen açılış "boru yok" görüp ayrı süreç başlatmasın
            let next = create_pipe(&name, false);
            if connected {
                answer(unsafe { std::fs::File::from_raw_handle(h) }, &ctx);
            } else {
                unsafe { CloseHandle(h) };
            }
            if next == INVALID_HANDLE_VALUE {
                return;
            }
            h = next;
        }
    });
    true
}

fn answer(mut pipe: std::fs::File, ctx: &egui::Context) {
    let mut len = [0u8; 4];
    if pipe.read_exact(&mut len).is_err() {
        return;
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > 64 * 1024 {
        return;
    }
    let mut buf = vec![0u8; len];
    if pipe.read_exact(&mut buf).is_err() {
        return;
    }
    let dir: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let hwnd = PARKED.swap(0, SeqCst);
    if hwnd != 0 {
        *REOPEN.lock().unwrap() = Some((!dir.is_empty()).then(|| PathBuf::from(OsString::from_wide(&dir))));
        // İlk kare eski içeriği göstermesin; arayüz yeni içerik hazır olunca açar
        crate::shell::set_cloak(hwnd, true);
        let h = hwnd as windows_sys::Win32::Foundation::HWND;
        unsafe {
            ShowWindow(h, if IsIconic(h) != 0 { SW_RESTORE } else { SW_SHOW });
            SetForegroundWindow(h);
        }
        ctx.request_repaint();
    }
    // Pencere açıksa reddedilir; yeni açılış kendi penceresini açar
    let _ = pipe.write_all(&[(hwnd != 0) as u8]);
    let _ = pipe.sync_all();
}

/// Pencereyi gizler ve sonraki açılışı bekler
pub fn park(hwnd: isize) {
    unsafe { ShowWindow(hwnd as windows_sys::Win32::Foundation::HWND, SW_HIDE) };
    PARKED.store(hwnd, SeqCst);
}

/// Gizliyken gelen açılış isteği
pub fn take_reopen() -> Option<Option<PathBuf>> {
    REOPEN.lock().unwrap().take()
}
