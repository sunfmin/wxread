//! Finding the running WeChat's process, so `wxread keys` can point Frida at
//! it (docs/adr/0005-keys-from-the-crypto-call.md). This is all wxread itself
//! reaches into WeChat for; the key capture is Frida's, run by [`crate::capture`].

/// The names WeChat for Mac's main process goes by (the International build
/// and the Chinese build).
#[cfg(target_os = "macos")]
const WECHAT_NAMES: [&str; 2] = ["WeChat", "Weixin"];

/// The pid of the running WeChat, if one is running: the first process whose
/// program file is named WeChat or Weixin.
#[cfg(target_os = "macos")]
pub fn find_wechat() -> Option<i32> {
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return None;
    }
    let mut pids = vec![0i32; count as usize + 32];
    let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
    let got = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    if got <= 0 {
        return None;
    }
    pids.truncate(got as usize);
    pids.into_iter().find(|&pid| pid > 0 && program_name(pid).is_some_and(|n| is_wechat(&n)))
}

/// Whether a program file named `name` is WeChat's main process.
#[cfg(target_os = "macos")]
fn is_wechat(name: &str) -> bool {
    WECHAT_NAMES.contains(&name)
}

/// The file name of process `pid`'s program, from its full path.
#[cfg(target_os = "macos")]
fn program_name(pid: i32) -> Option<String> {
    let mut buf = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    let path = std::path::Path::new(std::str::from_utf8(&buf[..n as usize]).ok()?);
    Some(path.file_name()?.to_string_lossy().into_owned())
}

/// Off macOS there is no WeChat for Mac to find
/// (docs/adr/0001-macos-only-and-apache-2.md).
#[cfg(not(target_os = "macos"))]
pub fn find_wechat() -> Option<i32> {
    None
}
