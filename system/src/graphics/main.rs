//! Native display and input smoke test for the future Linos graphics runtime.
//! This temporary X11/XWayland adapter targets ChromeOS Crostini development.
//! It is NOT a compositor, a final Linos UI toolkit or a bootable Desktop edition.
//! Requires the system X11 runtime library (libX11.so.6).
use std::{
    env,
    ffi::{CString, c_char, c_int, c_long, c_uint, c_ulong, c_void},
    mem::MaybeUninit,
    ptr,
};

type Display = c_void;
type Gc = *mut c_void;
type Window = c_ulong;

#[repr(C)]
#[derive(Default)]
struct WindowAttributes {
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border_width: c_int,
    depth: c_int,
    visual: *mut c_void,
    root: Window,
    class: c_int,
    bit_gravity: c_int,
    win_gravity: c_int,
    backing_store: c_int,
    backing_planes: c_ulong,
    backing_pixel: c_ulong,
    save_under: c_int,
    colormap: c_ulong,
    map_installed: c_int,
    map_state: c_int,
    all_event_masks: c_long,
    your_event_mask: c_long,
    do_not_propagate_mask: c_long,
    override_redirect: c_int,
    screen: *mut c_void,
}

#[link(name = ":libX11.so.6")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut Display;
    fn XCloseDisplay(display: *mut Display) -> c_int;
    fn XDefaultScreen(display: *mut Display) -> c_int;
    fn XRootWindow(display: *mut Display, screen: c_int) -> Window;
    fn XBlackPixel(display: *mut Display, screen: c_int) -> c_ulong;
    fn XWhitePixel(display: *mut Display, screen: c_int) -> c_ulong;
    fn XCreateSimpleWindow(
        display: *mut Display, parent: Window, x: c_int, y: c_int,
        width: c_uint, height: c_uint, border_width: c_uint,
        border: c_ulong, background: c_ulong,
    ) -> Window;
    fn XStoreName(display: *mut Display, window: Window, name: *const c_char) -> c_int;
    fn XSelectInput(display: *mut Display, window: Window, mask: c_long) -> c_int;
    fn XMapWindow(display: *mut Display, window: Window) -> c_int;
    fn XDestroyWindow(display: *mut Display, window: Window) -> c_int;
    fn XFlush(display: *mut Display) -> c_int;
    fn XNextEvent(display: *mut Display, event: *mut c_void) -> c_int;
    fn XCreateGC(display: *mut Display, window: Window, mask: c_ulong, values: *mut c_void) -> Gc;
    fn XFreeGC(display: *mut Display, gc: Gc) -> c_int;
    fn XSetForeground(display: *mut Display, gc: Gc, pixel: c_ulong) -> c_int;
    fn XFillRectangle(
        display: *mut Display, window: Window, gc: Gc,
        x: c_int, y: c_int, width: c_uint, height: c_uint,
    ) -> c_int;
    fn XDrawString(
        display: *mut Display, window: Window, gc: Gc,
        x: c_int, y: c_int, string: *const c_char, length: c_int,
    ) -> c_int;
    fn XGetWindowAttributes(
        display: *mut Display, window: Window, attrs: *mut WindowAttributes,
    ) -> c_int;
    fn XInternAtom(display: *mut Display, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XSetWMProtocols(display: *mut Display, window: Window, atoms: *mut c_ulong, count: c_int) -> c_int;
}

fn fill(display: *mut Display, window: Window, gc: Gc, color: c_ulong, rect: (i32, i32, u32, u32)) {
    // SAFETY: Called only while the X display, window and graphics context are valid.
    unsafe {
        XSetForeground(display, gc, color);
        XFillRectangle(display, window, gc, rect.0, rect.1, rect.2, rect.3);
    }
}

fn text(display: *mut Display, window: Window, gc: Gc, x: i32, y: i32, color: c_ulong, message: &str) {
    let bytes = message.as_bytes();
    // SAFETY: XDrawString reads exactly bytes.len() bytes and does not retain the pointer.
    unsafe {
        XSetForeground(display, gc, color);
        XDrawString(display, window, gc, x, y, bytes.as_ptr().cast(), bytes.len() as c_int);
    }
}

fn paint(display: *mut Display, window: Window, gc: Gc, clicks: u32) {
    let mut attrs = MaybeUninit::<WindowAttributes>::uninit();
    // SAFETY: XGetWindowAttributes initializes attrs on success.
    if unsafe { XGetWindowAttributes(display, window, attrs.as_mut_ptr()) } == 0 {
        return;
    }
    // SAFETY: XGetWindowAttributes succeeded above.
    let attrs = unsafe { attrs.assume_init() };
    let width = attrs.width.max(1) as u32;
    let height = attrs.height.max(1) as u32;
    const CANVAS: c_ulong = 0x101820;
    const PANEL: c_ulong = 0x202c36;
    const ACCENT: c_ulong = 0x60d6dc;
    const TEXT: c_ulong = 0xf2f6f8;
    const MUTED: c_ulong = 0x9babb5;
    fill(display, window, gc, CANVAS, (0, 0, width, height));
    fill(display, window, gc, PANEL, (0, 0, width, 64));
    fill(display, window, gc, ACCENT, (28, 23, 14, 14));
    text(display, window, gc, 54, 33, TEXT, "Linos 13 | Native graphics experiment");
    text(display, window, gc, 28, 108, TEXT, "Made to feel at home.");
    text(display, window, gc, 28, 143, MUTED, "Native Linux window, not HTML, not the final desktop.");
    text(display, window, gc, 28, 174, MUTED, "Resize the window, click anywhere, or press a key.");
    fill(display, window, gc, PANEL, (28, 208, width.saturating_sub(56), 105));
    text(display, window, gc, 46, 242, ACCENT, "Display and input");
    text(display, window, gc, 46, 267, TEXT, &format!("Surface: {width} x {height} px"));
    text(display, window, gc, 46, 291, TEXT, &format!("Mouse clicks: {clicks}"));
    if height > 350 {
        text(display, window, gc, 28, height as i32 - 26, MUTED, "Esc or window close to exit. X11 adapter via ChromeOS.");
    }
    // SAFETY: X display remains alive for the entirety of paint.
    unsafe { XFlush(display); }
}

fn run() -> Result<(), String> {
    if env::args().skip(1).any(|arg| arg == "--help" || arg == "-h") {
        println!("linos-graphics: first native graphical window for Linos 13 Desktop development");
        println!("Run from ChromeOS Linux with DISPLAY set; press Esc or close the window to exit.");
        return Ok(());
    }
    if env::var_os("DISPLAY").is_none() {
        return Err("DISPLAY is unavailable; this initial adapter uses X11/XWayland".into());
    }
    // SAFETY: Xlib calls are made exclusively on this thread. Resources are destroyed
    // before the display closes, and events are allocated with XEvent's ABI size.
    unsafe {
        let display = XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("Could not open X11/XWayland display; verify DISPLAY and ChromeOS Linux GUI support".into());
        }
        let screen = XDefaultScreen(display);
        let window = XCreateSimpleWindow(
            display, XRootWindow(display, screen), 80, 80, 760, 480, 0,
            XBlackPixel(display, screen), XWhitePixel(display, screen),
        );
        if window == 0 {
            XCloseDisplay(display);
            return Err("Could not create native window".into());
        }
        let title = CString::new("Linos 13 - Graphics laboratory").map_err(|e| e.to_string())?;
        XStoreName(display, window, title.as_ptr());
        const KEY_PRESS: c_long = 1;
        const BUTTON_PRESS: c_long = 1 << 2;
        const EXPOSURE: c_long = 1 << 15;
        const STRUCTURE: c_long = 1 << 17;
        XSelectInput(display, window, KEY_PRESS | BUTTON_PRESS | EXPOSURE | STRUCTURE);
        let atom = CString::new("WM_DELETE_WINDOW").map_err(|e| e.to_string())?;
        let mut close_atom = XInternAtom(display, atom.as_ptr(), 0);
        XSetWMProtocols(display, window, &mut close_atom, 1);
        let gc = XCreateGC(display, window, 0, ptr::null_mut());
        if gc.is_null() {
            XDestroyWindow(display, window);
            XCloseDisplay(display);
            return Err("Could not allocate graphics context".into());
        }
        XMapWindow(display, window);
        let mut clicks: u32 = 0;
        loop {
            // XEvent is 24 native longs on Linux. The first field is the event type.
            let mut event = [0 as c_long; 24];
            XNextEvent(display, event.as_mut_ptr().cast());
            match event[0] as c_int {
                12 | 22 => paint(display, window, gc, clicks), // Expose, ConfigureNotify
                4 => { // ButtonPress
                    clicks = clicks.saturating_add(1);
                    paint(display, window, gc, clicks);
                }
                2 | 33 | 17 => break, // KeyPress, WM close, DestroyNotify
                _ => {}
            }
        }
        XFreeGC(display, gc);
        XDestroyWindow(display, window);
        XCloseDisplay(display);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Linos graphics: {error}");
        std::process::exit(1);
    }
}
