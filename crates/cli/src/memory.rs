//! Reading the running WeChat's memory on macOS: finding its process and,
//! as root, scanning its readable-and-writable regions for the key literals
//! WeChat holds (docs/adr/0003-keys-from-memory.md). This is the one piece of
//! wxread that reaches into another process; it reads, and changes nothing.
//!
//! The pure crate [`wxread_crack`] reads the keys out of the bytes; this
//! module's only job is to hand it those bytes and to say, when it cannot,
//! why: no WeChat is running, or macOS refused the task port.

use wxread_crack::Access;

/// The names WeChat for Mac's main process goes by (the International build
/// and the Chinese build).
#[cfg(target_os = "macos")]
const WECHAT_NAMES: [&str; 2] = ["WeChat", "Weixin"];

/// The access wxread has to WeChat's memory, and every distinct 32-byte key
/// read out of it. Called only as root; `Access::NotRoot` is decided above.
#[cfg(target_os = "macos")]
pub fn probe() -> (Access, Vec<[u8; wxread_crack::KEY_LEN]>) {
    match find_wechat() {
        None => (Access::NotRunning, Vec::new()),
        Some(pid) => match read(pid) {
            Ok(candidates) => (Access::Read, candidates),
            Err(why) => (Access::Refused(why), Vec::new()),
        },
    }
}

/// The pid of the running WeChat, if one is running: the first process whose
/// program file is named WeChat or Weixin.
#[cfg(target_os = "macos")]
fn find_wechat() -> Option<i32> {
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

/// Every distinct 32-byte key in the readable-and-writable memory of process
/// `pid`, or why its memory could not be read.
#[cfg(target_os = "macos")]
fn read(pid: i32) -> Result<Vec<[u8; wxread_crack::KEY_LEN]>, String> {
    use std::collections::HashSet;

    use mach2::kern_return::KERN_SUCCESS;
    use mach2::port::mach_port_t;
    use mach2::traps::{mach_task_self, task_for_pid};
    use mach2::vm::mach_vm_region;
    use mach2::vm_prot::{VM_PROT_READ, VM_PROT_WRITE};
    use mach2::vm_region::{
        VM_REGION_BASIC_INFO_64, vm_region_basic_info_64, vm_region_basic_info_data_64_t,
        vm_region_info_t,
    };
    use mach2::vm_types::{mach_vm_address_t, mach_vm_size_t};

    let mut task: mach_port_t = 0;
    let kr = unsafe { task_for_pid(mach_task_self(), pid, &mut task) };
    if kr != KERN_SUCCESS {
        return Err(format!("task_for_pid returned {kr}"));
    }

    let mut keys: HashSet<[u8; wxread_crack::KEY_LEN]> = HashSet::new();
    let mut addr: mach_vm_address_t = 0;
    loop {
        let mut size: mach_vm_size_t = 0;
        let mut info = vm_region_basic_info_data_64_t::default();
        let mut info_count = vm_region_basic_info_64::count();
        let mut object_name: mach_port_t = 0;
        let kr = unsafe {
            mach_vm_region(
                task,
                &mut addr,
                &mut size,
                VM_REGION_BASIC_INFO_64,
                (&mut info as *mut vm_region_basic_info_data_64_t).cast::<i32>()
                    as vm_region_info_t,
                &mut info_count,
                &mut object_name,
            )
        };
        if kr != KERN_SUCCESS {
            break;
        }
        if size == 0 {
            addr = addr.saturating_add(1);
            continue;
        }
        let rw = VM_PROT_READ | VM_PROT_WRITE;
        if info.protection & rw == rw {
            scan(task, addr, size, &mut keys);
        }
        addr = addr.saturating_add(size);
    }
    Ok(keys.into_iter().collect())
}

/// Reads one region, `base` for `size` bytes, in chunks, and adds every key
/// in it to `keys`. Chunks overlap by the length of WeChat's literal less
/// one, so a key split across a chunk boundary is still read whole.
#[cfg(target_os = "macos")]
fn scan(
    task: mach2::port::mach_port_t,
    base: mach2::vm_types::mach_vm_address_t,
    size: mach2::vm_types::mach_vm_size_t,
    keys: &mut std::collections::HashSet<[u8; wxread_crack::KEY_LEN]>,
) {
    use mach2::kern_return::KERN_SUCCESS;
    use mach2::vm::mach_vm_read_overwrite;
    use mach2::vm_types::mach_vm_size_t;

    const CHUNK: u64 = 2 * 1024 * 1024;
    // x'<64 hex key><32 hex salt>' is 99 bytes; keep that much, less one, so
    // no literal is cut in two between chunks.
    const OVERLAP: u64 = 99;

    let mut buf = vec![0u8; CHUNK as usize];
    let end = base.saturating_add(size);
    let mut at = base;
    while at < end {
        let want = (end - at).min(CHUNK);
        let mut got: mach_vm_size_t = 0;
        let kr =
            unsafe { mach_vm_read_overwrite(task, at, want, buf.as_mut_ptr() as u64, &mut got) };
        if kr == KERN_SUCCESS {
            let got = (got as usize).min(buf.len());
            for key in wxread_crack::candidates(&buf[..got]) {
                keys.insert(key);
            }
        }
        at = at.saturating_add(if want > OVERLAP { want - OVERLAP } else { want });
    }
}

/// Off macOS there is no WeChat to read (docs/adr/0001-macos-only-and-apache-2.md).
#[cfg(not(target_os = "macos"))]
pub fn probe() -> (Access, Vec<[u8; wxread_crack::KEY_LEN]>) {
    (
        Access::Refused("wxread keys reads WeChat for Mac, and this is not a macOS build".into()),
        Vec::new(),
    )
}
