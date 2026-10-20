//! WeChat's window, driven as a person drives it: what macOS allows the app
//! running wxread, the running WeChat and the accounts it has open, its
//! window and the text macOS recognizes in a screenshot of it, and the
//! keys, menus, clicks and pasteboard a leave goes through
//! (docs/adr/0015-a-group-is-left-by-driving-wechats-window-only-with-yes.md,
//! docs/leave-group.md). Vision, CoreGraphics, Accessibility, IOKit and
//! AppKit are called through their C and Objective-C interfaces, with no
//! helper beside the binary; [`wxread_leave`] decides what is done with
//! them. Nothing here reads WeChat's databases.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr::{self, NonNull};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, bail};
use objc2::encode::{Encode, Encoding};
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use wxread_doctor::Permission;
use wxread_leave::{Act, Frame, Key, Line, MenuItem, Overlay, Screen};

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFArrayRef = *const c_void;
type CFDictionaryRef = *const c_void;
type AXUIElementRef = *const c_void;
type CGEventRef = *mut c_void;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(b: CFTypeRef) -> u8;
    fn CFStringGetTypeID() -> usize;
    fn CFStringCreateWithBytes(
        alloc: CFTypeRef,
        bytes: *const u8,
        len: isize,
        encoding: u32,
        external: u8,
    ) -> CFStringRef;
    fn CFStringGetLength(s: CFStringRef) -> isize;
    fn CFStringGetMaximumSizeForEncoding(len: isize, encoding: u32) -> isize;
    fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFArrayGetCount(a: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFArrayRef, i: isize) -> CFTypeRef;
    fn CFDictionaryGetValue(d: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;
    fn CFNumberGetValue(n: CFTypeRef, kind: isize, out: *mut c_void) -> u8;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    static kCGWindowOwnerPID: CFStringRef;
    static kCGWindowNumber: CFStringRef;
    static kCGWindowLayer: CFStringRef;
    static kCGWindowBounds: CFStringRef;
    static kCGWindowName: CFStringRef;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGWindowListCopyWindowInfo(option: u32, relative: u32) -> CFArrayRef;
    fn CGRectMakeWithDictionaryRepresentation(d: CFDictionaryRef, rect: *mut CGRect) -> bool;
    fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CGEventRef;
    fn CGEventCreateMouseEvent(
        source: CFTypeRef,
        kind: u32,
        at: CGPoint,
        button: u32,
    ) -> CGEventRef;
    fn CGEventCreateScrollWheelEvent2(
        source: CFTypeRef,
        units: u32,
        count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFDictionaryRef;
    fn IOServiceGetMatchingService(port: u32, matching: CFDictionaryRef) -> u32;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: CFStringRef,
        alloc: CFTypeRef,
        options: u32,
    ) -> CFTypeRef;
    fn IOObjectRelease(object: u32) -> i32;
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSPasteboardTypeString: &'static AnyObject;
}

#[link(name = "Vision", kind = "framework")]
unsafe extern "C" {}

const UTF8: u32 = 0x0800_0100;
const NUMBER_SINT64: isize = 4;
const ON_SCREEN_ONLY: u32 = 1;
const HID_EVENT_TAP: u32 = 0;
const LEFT_MOUSE_DOWN: u32 = 1;
const LEFT_MOUSE_UP: u32 = 2;
const MOUSE_MOVED: u32 = 5;
const SCROLL_LINES: u32 = 1;
/// How far the wheel turns per scroll, in lines: down the chat info.
const SCROLL_BY: i32 = -10;
const KEY_ESCAPE: u16 = 53;
/// What WeChat's main window is named, in English and in Chinese: its
/// others (a file it shows, the search of the internet) are named for
/// what they show.
const MAIN_TITLES: [&str; 3] = ["Weixin", "WeChat", "微信"];
/// The executables WeChat for Mac runs as.
const WECHAT_EXECUTABLES: [&str; 2] = ["/Contents/MacOS/WeChat", "/Contents/MacOS/Weixin"];

/// A Core Foundation object this module owns, released when dropped.
struct Owned(CFTypeRef);

impl Owned {
    fn new(cf: CFTypeRef) -> Option<Owned> {
        (!cf.is_null()).then_some(Owned(cf))
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

/// `text` as a CFString.
fn cf_string(text: &str) -> Owned {
    let s = unsafe {
        CFStringCreateWithBytes(ptr::null(), text.as_ptr(), text.len() as isize, UTF8, 0)
    };
    Owned::new(s).expect("a CFString of UTF-8 text")
}

/// The text of `s`, when it is a CFString.
fn rust_string(s: CFTypeRef) -> Option<String> {
    if s.is_null() || unsafe { CFGetTypeID(s) != CFStringGetTypeID() } {
        return None;
    }
    let size = unsafe { CFStringGetMaximumSizeForEncoding(CFStringGetLength(s), UTF8) } + 1;
    let mut buf = vec![0 as c_char; usize::try_from(size).ok()?];
    if unsafe { CFStringGetCString(s, buf.as_mut_ptr(), size, UTF8) } == 0 {
        return None;
    }
    Some(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
}

fn number(n: CFTypeRef) -> Option<i64> {
    let mut v: i64 = 0;
    (!n.is_null() && unsafe { CFNumberGetValue(n, NUMBER_SINT64, (&raw mut v).cast()) } != 0)
        .then_some(v)
}

/// The permissions [`Permission`] names that macOS has not given the app
/// running wxread. Asking changes nothing: neither puts up a prompt.
pub fn missing() -> Vec<Permission> {
    Permission::ALL.into_iter().filter(|p| !granted(*p)).collect()
}

/// Whether macOS gives the app running wxread `permission`.
pub fn granted(permission: Permission) -> bool {
    match permission {
        Permission::Accessibility => unsafe { AXIsProcessTrusted() },
        Permission::ScreenRecording => unsafe { CGPreflightScreenCaptureAccess() },
    }
}

/// The running WeChat's process, if one runs.
pub fn wechat() -> Option<i32> {
    let count = unsafe { libc::proc_listallpids(ptr::null_mut(), 0) };
    let mut pids = vec![0 as libc::pid_t; usize::try_from(count).ok()? + 64];
    let size = i32::try_from(pids.len() * size_of::<libc::pid_t>()).ok()?;
    let n = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
    pids.truncate(usize::try_from(n).ok()?);
    pids.into_iter().find(|&pid| {
        path_of(pid).is_some_and(|p| WECHAT_EXECUTABLES.iter().any(|e| p.ends_with(e)))
    })
}

/// The executable process `pid` runs.
fn path_of(pid: i32) -> Option<String> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    let n = usize::try_from(n).ok().filter(|n| *n > 0)?;
    buf.truncate(n);
    String::from_utf8(buf).ok()
}

/// `proc_fileinfo` and `vnode_info_path` together, as
/// `PROC_PIDFDVNODEPATHINFO` answers them (sys/proc_info.h).
#[repr(C)]
struct VnodeFdInfoWithPath {
    openflags: u32,
    status: u32,
    offset: i64,
    kind: i32,
    guardflags: u32,
    path: libc::vnode_info_path,
}

const PROC_PIDFDVNODEPATHINFO: i32 = 2;

/// The accounts under `data_dir` whose databases process `pid` has open,
/// by their directory's name: the account WeChat is logged in as.
pub fn accounts_open(pid: i32, data_dir: &Path) -> Vec<String> {
    let dir = data_dir.canonicalize().unwrap_or_else(|_| data_dir.to_path_buf());
    let mut accounts: Vec<String> = open_files(pid)
        .iter()
        .filter_map(|file| {
            let under = file.strip_prefix(&dir).ok()?;
            let mut parts = under.components();
            let account = parts.next()?.as_os_str().to_str()?.to_string();
            (parts.next()?.as_os_str() == wxread_doctor::DB_STORAGE).then_some(account)
        })
        .collect();
    accounts.sort();
    accounts.dedup();
    accounts
}

/// Every file process `pid` has open.
fn open_files(pid: i32) -> Vec<PathBuf> {
    let size = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, ptr::null_mut(), 0) };
    let Ok(size) = usize::try_from(size) else { return Vec::new() };
    let mut fds = vec![libc::proc_fdinfo { proc_fd: 0, proc_fdtype: 0 }; size / 8 + 32];
    let bytes = (fds.len() * size_of::<libc::proc_fdinfo>()) as i32;
    let n = unsafe {
        libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, fds.as_mut_ptr().cast(), bytes)
    };
    fds.truncate(usize::try_from(n).unwrap_or(0) / size_of::<libc::proc_fdinfo>());
    let mut files = Vec::new();
    for fd in fds.iter().filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_VNODE as u32) {
        let mut info = std::mem::MaybeUninit::<VnodeFdInfoWithPath>::zeroed();
        let size = size_of::<VnodeFdInfoWithPath>() as i32;
        let got = unsafe {
            libc::proc_pidfdinfo(
                pid,
                fd.proc_fd,
                PROC_PIDFDVNODEPATHINFO,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if got != size {
            continue;
        }
        let info = unsafe { info.assume_init() };
        let path: &[c_char] = info.path.vip_path.as_flattened();
        let path = unsafe { CStr::from_ptr(path.as_ptr()) };
        files.push(PathBuf::from(path.to_string_lossy().into_owned()));
    }
    files
}

/// Seconds since the person at the Mac last touched a key, the mouse or
/// the trackpad.
pub fn idle() -> f64 {
    let name = CString::new("IOHIDSystem").unwrap();
    let service = unsafe { IOServiceGetMatchingService(0, IOServiceMatching(name.as_ptr())) };
    if service == 0 {
        return 0.0;
    }
    let key = cf_string("HIDIdleTime");
    let value = unsafe { IORegistryEntryCreateCFProperty(service, key.0, ptr::null(), 0) };
    unsafe { IOObjectRelease(service) };
    let value = Owned::new(value);
    let nanos = value.and_then(|v| number(v.0)).unwrap_or(0);
    nanos as f64 / 1e9
}

/// WeChat's process and its main window, with the window's frame on the
/// screen, in points.
pub struct Window {
    pid: i32,
    id: u32,
    frame: CGRect,
}

/// One window on screen: its number, layer, name and frame.
struct Shown {
    id: u32,
    layer: i64,
    name: String,
    frame: CGRect,
}

/// The windows on screen of process `pid`.
fn windows(pid: i32) -> Vec<Shown> {
    let Some(list) = Owned::new(unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY, 0) }) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for i in 0..unsafe { CFArrayGetCount(list.0) } {
        let w = unsafe { CFArrayGetValueAtIndex(list.0, i) };
        let get = |key: CFStringRef| unsafe { CFDictionaryGetValue(w, key) };
        if number(get(unsafe { kCGWindowOwnerPID })) != Some(i64::from(pid)) {
            continue;
        }
        let (Some(id), Some(layer)) =
            (number(get(unsafe { kCGWindowNumber })), number(get(unsafe { kCGWindowLayer })))
        else {
            continue;
        };
        let mut frame = CGRect::default();
        let bounds = get(unsafe { kCGWindowBounds });
        if bounds.is_null()
            || !unsafe { CGRectMakeWithDictionaryRepresentation(bounds, &mut frame) }
        {
            continue;
        }
        let name = rust_string(get(unsafe { kCGWindowName })).unwrap_or_default();
        found.push(Shown { id: id as u32, layer, name, frame });
    }
    found
}

impl Window {
    /// WeChat's main window: the one named as [`MAIN_TITLES`] say, else
    /// the largest of its windows on screen at layer 0.
    pub fn of(pid: i32) -> anyhow::Result<Window> {
        let area = |r: &CGRect| r.size.width * r.size.height;
        let shown: Vec<Shown> = windows(pid).into_iter().filter(|w| w.layer == 0).collect();
        let main = shown
            .iter()
            .find(|w| MAIN_TITLES.contains(&w.name.as_str()))
            .or_else(|| shown.iter().max_by(|a, b| area(&a.frame).total_cmp(&area(&b.frame))));
        match main {
            Some(w) => Ok(Window { pid, id: w.id, frame: w.frame }),
            None => bail!(
                "WeChat has no window on screen: open its window (click WeChat in the Dock), then run it again"
            ),
        }
    }

    /// WeChat's other windows inside the main one (the matches a search
    /// lists, a confirmation), each with its frame in the main window's
    /// points.
    fn overlays(&self) -> Vec<(u32, CGRect, Frame)> {
        let main = self.frame;
        let mut found = Vec::new();
        for w in windows(self.pid) {
            let r = w.frame;
            let inside = w.id != self.id
                && r.origin.x >= main.origin.x
                && r.origin.y >= main.origin.y
                && r.origin.x + r.size.width <= main.origin.x + main.size.width
                && r.origin.y + r.size.height <= main.origin.y + main.size.height;
            if inside {
                let frame = Frame {
                    x: points(r.origin.x - main.origin.x),
                    y: points(r.origin.y - main.origin.y),
                    w: points(r.size.width),
                    h: points(r.size.height),
                };
                found.push((w.id, r, frame));
            }
        }
        found
    }

    /// The window read: every line of text macOS recognizes in a
    /// screenshot of it, and in one of each of WeChat's windows over it.
    /// Each screenshot goes to a directory only this user can read,
    /// removed as soon as it is read.
    pub fn look(&mut self) -> anyhow::Result<Screen> {
        // The window may have moved since the last look.
        *self = Window::of(self.pid)?;
        let dir = private_dir()?;
        let read = (|| {
            let lines = shot(&dir, self.id, self.frame.size)?.ok_or_else(|| {
                anyhow!("the screenshot of WeChat's window is not of its window: run it again")
            })?;
            let mut overlays = Vec::new();
            for (id, r, frame) in self.overlays() {
                let mut lines = shot(&dir, id, r.size)?.unwrap_or_default();
                for line in &mut lines {
                    line.x = line.x.saturating_add(frame.x);
                    line.y = line.y.saturating_add(frame.y);
                }
                overlays.push(Overlay { frame, lines });
            }
            Ok::<_, anyhow::Error>((lines, overlays))
        })();
        let _ = std::fs::remove_dir_all(&dir);
        let (lines, overlays) = read?;
        Ok(Screen {
            width: points(self.frame.size.width),
            height: points(self.frame.size.height),
            lines,
            overlays,
        })
    }

    /// Does `act` to the window.
    pub fn act(&self, act: &Act) -> anyhow::Result<()> {
        match act {
            Act::Front => front(self.pid),
            Act::Key(key) => {
                key_press(match key {
                    Key::Escape => KEY_ESCAPE,
                });
                Ok(())
            }
            Act::Menu(item) => menu(self.pid, *item),
            Act::Paste(text) => paste(self.pid, text),
            Act::Click { x, y } => {
                // A click on a window of an app not in front only brings it there.
                if !frontmost(self.pid) {
                    front(self.pid)?;
                }
                click(self.at(*x, *y));
                Ok(())
            }
            Act::Scroll { x, y } => {
                scroll(self.at(*x, *y));
                Ok(())
            }
            Act::Wait(ms) => {
                sleep(Duration::from_millis(*ms));
                Ok(())
            }
        }
    }

    /// A point of the window, on the screen.
    fn at(&self, x: u32, y: u32) -> CGPoint {
        CGPoint { x: self.frame.origin.x + f64::from(x), y: self.frame.origin.y + f64::from(y) }
    }
}

/// `v` in whole points, never below zero.
fn points(v: f64) -> u32 {
    v.round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// The text recognized in a screenshot of window `id`, `size` points big,
/// taken into `dir`; none when the screenshot is not of that window alone.
/// A confirmation's own window is shot as all of WeChat's, squeezed into
/// its frame: its text is then read in the main window's screenshot.
fn shot(dir: &Path, id: u32, size: CGSize) -> anyhow::Result<Option<Vec<Line>>> {
    let file = dir.join(format!("{id}.png"));
    let out = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-o", "-l", &id.to_string()])
        .arg(&file)
        .output()
        .map_err(|e| anyhow!("screencapture could not be run ({e}): run it again"))?;
    if !out.status.success() || !file.exists() {
        let p = Permission::ScreenRecording;
        bail!("{}: {}", p.problem(), p.fix());
    }
    let bytes = std::fs::read(&file)?;
    if !shot_of(&bytes, size) {
        return Ok(None);
    }
    recognize(&file, size).map(Some)
}

/// Whether the PNG `png` is an image of a window `size` points big: its
/// pixels the same multiple of its points across and down.
fn shot_of(png: &[u8], size: CGSize) -> bool {
    let dimension = |at: usize| {
        png.get(at..at + 4).map(|b| f64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
    };
    // The IHDR chunk, right after the signature, holds the width and height.
    let (Some(w), Some(h)) = (dimension(16), dimension(20)) else { return false };
    if size.width < 1.0 || size.height < 1.0 {
        return false;
    }
    let (across, down) = (w / size.width, h / size.height);
    (across - down).abs() <= across * 0.05
}

/// A new directory only this user can read, in the temporary directory.
fn private_dir() -> anyhow::Result<PathBuf> {
    let template = std::env::temp_dir().join("wxread-leave-XXXXXX");
    let template = CString::new(template.to_string_lossy().into_owned())?;
    let mut bytes = template.into_bytes_with_nul();
    let made = unsafe { libc::mkdtemp(bytes.as_mut_ptr().cast()) };
    if made.is_null() {
        bail!(
            "no directory could be made in {} for the window's screenshot: set TMPDIR to a directory wxread may write, then run it again",
            std::env::temp_dir().display()
        );
    }
    bytes.pop();
    Ok(PathBuf::from(String::from_utf8(bytes)?))
}

/// The text macOS's Vision recognizes in the image at `file`, accurately,
/// in Chinese and English, each line where it is in a window of `size`
/// points.
fn recognize(file: &Path, size: CGSize) -> anyhow::Result<Vec<Line>> {
    autoreleasepool(|_| unsafe {
        let path = ns_string(&file.to_string_lossy());
        let url: Retained<AnyObject> = msg_send![class!(NSURL), fileURLWithPath: &*path];
        let options: Retained<AnyObject> = msg_send![class!(NSDictionary), dictionary];
        let handler: Retained<AnyObject> = msg_send![
            msg_send![class!(VNImageRequestHandler), alloc],
            initWithURL: &*url,
            options: &*options
        ];
        let request: Retained<AnyObject> = msg_send![class!(VNRecognizeTextRequest), new];
        // VNRequestTextRecognitionLevelAccurate
        let _: () = msg_send![&*request, setRecognitionLevel: 0isize];
        let _: () = msg_send![&*request, setUsesLanguageCorrection: false];
        let languages = ns_array(&[&*ns_string("zh-Hans"), &*ns_string("en-US")]);
        let _: () = msg_send![&*request, setRecognitionLanguages: &*languages];
        let requests = ns_array(&[&*request]);
        let mut error: *mut AnyObject = ptr::null_mut();
        let done: bool = msg_send![&*handler, performRequests: &*requests, error: &mut error];
        if !done {
            bail!("macOS could not read the text of WeChat's window: run it again");
        }
        let results: Option<Retained<AnyObject>> = msg_send![&*request, results];
        let mut lines = Vec::new();
        let Some(results) = results else { return Ok(lines) };
        let count: usize = msg_send![&*results, count];
        for i in 0..count {
            let seen: Retained<AnyObject> = msg_send![&*results, objectAtIndex: i];
            let top: Retained<AnyObject> = msg_send![&*seen, topCandidates: 1usize];
            let first: Option<Retained<AnyObject>> = msg_send![&*top, firstObject];
            let Some(first) = first else { continue };
            let text: Retained<AnyObject> = msg_send![&*first, string];
            // Normalized to the image, from its bottom left.
            let b: CGRect = msg_send![&*seen, boundingBox];
            lines.push(Line {
                text: from_ns_string(&text),
                x: points(b.origin.x * size.width),
                y: points((1.0 - b.origin.y - b.size.height) * size.height),
                w: points(b.size.width * size.width),
                h: points(b.size.height * size.height),
            });
        }
        Ok(lines)
    })
}

fn ns_string(text: &str) -> Retained<AnyObject> {
    let text = CString::new(text.replace('\0', "")).unwrap();
    unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
}

fn from_ns_string(s: &AnyObject) -> String {
    let p: *const c_char = unsafe { msg_send![s, UTF8String] };
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

fn ns_array(items: &[&AnyObject]) -> Retained<AnyObject> {
    let objects: Vec<NonNull<AnyObject>> = items.iter().map(|o| NonNull::from(*o)).collect();
    unsafe { msg_send![class!(NSArray), arrayWithObjects: objects.as_ptr(), count: objects.len()] }
}

/// Accessibility's attribute `name` of `element`, owned.
fn attribute(element: AXUIElementRef, name: &str) -> Option<Owned> {
    let name = cf_string(name);
    let mut value: CFTypeRef = ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(element, name.0, &mut value) };
    if err == 0 { Owned::new(value) } else { None }
}

/// Whether process `pid` is the active app, as Accessibility says: an
/// app's own idea of it is not updated in a process with no run loop.
fn frontmost(pid: i32) -> bool {
    let Some(app) = Owned::new(unsafe { AXUIElementCreateApplication(pid) }) else { return false };
    attribute(app.0, "AXFrontmost").is_some_and(|v| unsafe {
        CFGetTypeID(v.0) == CFBooleanGetTypeID() && CFBooleanGetValue(v.0) != 0
    })
}

/// Makes process `pid` the active app through Accessibility, and checks it
/// is: asking an app to activate itself from the background is not always
/// honored.
fn front(pid: i32) -> anyhow::Result<()> {
    let app = Owned::new(unsafe { AXUIElementCreateApplication(pid) })
        .ok_or_else(|| anyhow!("WeChat's process {pid} answers no Accessibility element"))?;
    let name = cf_string("AXFrontmost");
    for _ in 0..10 {
        unsafe { AXUIElementSetAttributeValue(app.0, name.0, kCFBooleanTrue) };
        sleep(Duration::from_millis(300));
        if frontmost(pid) {
            raise_main(app.0);
            return Ok(());
        }
    }
    bail!(
        "WeChat did not become the active app, so nothing was clicked: run it again with the Mac left alone, or leave the group in WeChat itself"
    )
}

/// Puts WeChat's main window (`app`'s) ahead of its others: its menus are
/// the main window's, and another (a file it shows) has others.
fn raise_main(app: AXUIElementRef) {
    let (_windows, windows) = elements(app, "AXWindows");
    let (raise, main) = (cf_string("AXRaise"), cf_string("AXMain"));
    for window in windows.into_iter().filter(|w| MAIN_TITLES.contains(&title(*w).as_str())) {
        unsafe {
            AXUIElementPerformAction(window, raise.0);
            AXUIElementSetAttributeValue(window, main.0, kCFBooleanTrue);
        }
    }
    sleep(Duration::from_millis(300));
}

/// The app in front now, to give the front back to.
pub fn front_app() -> Option<i32> {
    autoreleasepool(|_| unsafe {
        let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app: Option<Retained<AnyObject>> = msg_send![&*workspace, frontmostApplication];
        app.map(|app| msg_send![&*app, processIdentifier])
    })
}

/// Gives the front back to process `pid`, as far as it will take it.
pub fn give_front(pid: i32) {
    if let Some(app) = Owned::new(unsafe { AXUIElementCreateApplication(pid) }) {
        let name = cf_string("AXFrontmost");
        unsafe { AXUIElementSetAttributeValue(app.0, name.0, kCFBooleanTrue) };
    }
}

/// The elements an Accessibility element's attribute `name` lists, and the
/// list that owns them.
fn elements(element: AXUIElementRef, name: &str) -> (Option<Owned>, Vec<AXUIElementRef>) {
    let Some(list) = attribute(element, name) else { return (None, Vec::new()) };
    let count = unsafe { CFArrayGetCount(list.0) };
    let items = (0..count).map(|i| unsafe { CFArrayGetValueAtIndex(list.0, i) }).collect();
    (Some(list), items)
}

fn title(element: AXUIElementRef) -> String {
    attribute(element, "AXTitle").and_then(|t| rust_string(t.0)).unwrap_or_default()
}

/// Presses `item` of WeChat's menus through Accessibility: in English or in
/// Chinese, whichever WeChat shows.
fn menu(pid: i32, item: MenuItem) -> anyhow::Result<()> {
    let app = Owned::new(unsafe { AXUIElementCreateApplication(pid) })
        .ok_or_else(|| anyhow!("WeChat's process {pid} answers no Accessibility element"))?;
    let bar = attribute(app.0, "AXMenuBar").ok_or_else(|| {
        anyhow!("WeChat answers no menu bar to Accessibility: {}", Permission::Accessibility.fix())
    })?;
    let press = cf_string("AXPress");
    let (_menus, menus) = elements(bar.0, "AXChildren");
    for (menu_title, item_title) in item.titles() {
        for &menu in menus.iter().filter(|m| title(**m) == menu_title) {
            let (_lists, lists) = elements(menu, "AXChildren");
            for list in lists {
                let (_items, items) = elements(list, "AXChildren");
                if let Some(&found) = items.iter().find(|i| title(**i) == item_title) {
                    let err = unsafe { AXUIElementPerformAction(found, press.0) };
                    if err != 0 {
                        bail!(
                            "pressing WeChat's {menu_title} › {item_title} answered {err}: run it again"
                        );
                    }
                    return Ok(());
                }
            }
        }
    }
    let [(menu, name), _] = item.titles();
    bail!(
        "WeChat has no menu item {menu} › {name}: its menus are not WeChat 4.1.15's, so nothing was clicked"
    )
}

fn post(event: CGEventRef) {
    if !event.is_null() {
        unsafe {
            CGEventPost(HID_EVENT_TAP, event);
            CFRelease(event);
        }
        sleep(Duration::from_millis(30));
    }
}

fn key_press(key: u16) {
    for down in [true, false] {
        post(unsafe { CGEventCreateKeyboardEvent(ptr::null(), key, down) });
    }
}

fn click(at: CGPoint) {
    for kind in [MOUSE_MOVED, LEFT_MOUSE_DOWN, LEFT_MOUSE_UP] {
        post(unsafe { CGEventCreateMouseEvent(ptr::null(), kind, at, 0) });
    }
}

fn scroll(at: CGPoint) {
    post(unsafe { CGEventCreateMouseEvent(ptr::null(), MOUSE_MOVED, at, 0) });
    post(unsafe { CGEventCreateScrollWheelEvent2(ptr::null(), SCROLL_LINES, 1, SCROLL_BY, 0, 0) });
}

/// Puts `text` where WeChat's focus is by Edit › Paste: typed keys stay in
/// a Chinese input source's composition. The pasteboard holds `text` for
/// that moment only, and what it held is put back.
fn paste(pid: i32, text: &str) -> anyhow::Result<()> {
    autoreleasepool(|_| unsafe {
        let board: Retained<AnyObject> = msg_send![class!(NSPasteboard), generalPasteboard];
        let kept = pasteboard_items(&board);
        let _: isize = msg_send![&*board, clearContents];
        let set: bool =
            msg_send![&*board, setString: &*ns_string(text), forType: NSPasteboardTypeString];
        let pressed = if set {
            menu(pid, MenuItem::Paste)
        } else {
            Err(anyhow!("the name could not be put on the pasteboard: run it again"))
        };
        sleep(Duration::from_millis(300));
        let _: isize = msg_send![&*board, clearContents];
        if !kept.is_empty() {
            let items: Vec<&AnyObject> = kept.iter().map(|i| &**i).collect();
            let _: bool = msg_send![&*board, writeObjects: &*ns_array(&items)];
        }
        pressed
    })
}

/// Copies of what the pasteboard holds: each item with its data of each
/// type.
unsafe fn pasteboard_items(board: &AnyObject) -> Vec<Retained<AnyObject>> {
    let items: Option<Retained<AnyObject>> = unsafe { msg_send![board, pasteboardItems] };
    let Some(items) = items else { return Vec::new() };
    let count: usize = unsafe { msg_send![&*items, count] };
    let mut copies = Vec::new();
    for i in 0..count {
        unsafe {
            let item: Retained<AnyObject> = msg_send![&*items, objectAtIndex: i];
            let copy: Retained<AnyObject> = msg_send![class!(NSPasteboardItem), new];
            let types: Retained<AnyObject> = msg_send![&*item, types];
            let n: usize = msg_send![&*types, count];
            for t in 0..n {
                let kind: Retained<AnyObject> = msg_send![&*types, objectAtIndex: t];
                let data: Option<Retained<AnyObject>> = msg_send![&*item, dataForType: &*kind];
                if let Some(data) = data {
                    let _: bool = msg_send![&*copy, setData: &*data, forType: &*kind];
                }
            }
            copies.push(copy);
        }
    }
    copies
}
