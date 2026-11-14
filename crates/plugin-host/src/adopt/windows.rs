// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Adopting a program's window on Windows: the program's main window loses its title bar and
//! borders, becomes a child window and moves into the embedded view (`SetParent`, which works
//! across processes). Its own dialogs stay owned by it, so they show over YOBUKO's window.

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, IsZoomed, PostMessageW, SW_RESTORE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CLOSE,
    WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_STATICEDGE, WS_EX_WINDOWEDGE,
    WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
};

use crate::window::{Size, Window};

/// The program's main window, once found.
pub(crate) struct Program {
    hwnd: HWND,
    view: HWND,
}

/// The smallest window taken for a program's main one: a splash or a tooltip is smaller.
const SMALLEST: i32 = 200;

/// What the search through the screen's windows is after, and what it found.
struct Search<'a> {
    pid: u32,
    title: Option<&'a str>,
    best: Option<(i64, HWND)>,
}

/// Runs `f` aware of each monitor's scale, as YOBUKO's window is, so sizes are in its pixels.
fn per_monitor<R>(f: impl FnOnce() -> R) -> R {
    // SAFETY: a documented constant; the call returns the thread's context before, or null
    // where it is not supported, and then changes nothing.
    let before = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let result = f();
    if !before.is_null() {
        // SAFETY: the context this thread had, given back.
        unsafe { SetThreadDpiAwarenessContext(before) };
    }
    result
}

/// Looks at one top-level window for [`Program::find`].
unsafe extern "system" fn look(hwnd: HWND, search: LPARAM) -> i32 {
    // SAFETY: `find` passes a pointer to its `Search`, which outlives the enumeration, and
    // nothing else holds it meanwhile.
    let search = unsafe { &mut *(search as *mut Search<'_>) };
    let mut pid = 0u32;
    // SAFETY: a window the system just named, and a number to fill.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != search.pid {
        return 1;
    }
    // SAFETY: as above; only reads.
    let shown = unsafe { IsWindowVisible(hwnd) } != 0;
    // SAFETY: as above.
    let owner = unsafe { GetWindow(hwnd, GW_OWNER) };
    if !shown || !owner.is_null() {
        return 1;
    }
    let mut title = [0u16; 256];
    // SAFETY: the buffer and its length are this array's.
    let length = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    let title = String::from_utf16_lossy(title.get(..usize::try_from(length).unwrap_or(0)).unwrap_or(&[]));
    if !super::titled(&title, search.title) {
        return 1;
    }
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: as above, with a rectangle to fill.
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return 1;
    }
    let (width, height) = (rect.right.saturating_sub(rect.left), rect.bottom.saturating_sub(rect.top));
    if width >= SMALLEST && height >= SMALLEST {
        let area = i64::from(width) * i64::from(height);
        if search.best.is_none_or(|(most, _)| area > most) {
            search.best = Some((area, hwnd));
        }
    }
    1
}

impl Program {
    /// The main window of process `pid`: shown, owned by no other window (a dialog is), the
    /// largest if there are several.
    pub(crate) fn find(pid: u32, title: Option<&str>) -> Option<Self> {
        let mut search = Search { pid, title, best: None };
        // SAFETY: `look` reads the `Search` behind the pointer only during this call.
        unsafe { EnumWindows(Some(look), &mut search as *mut Search<'_> as LPARAM) };
        let (_, hwnd) = search.best?;
        Some(Self { hwnd, view: std::ptr::null_mut() })
    }

    /// Takes the window's title bar and borders away and puts it in `view`, at its top left
    /// corner and `size`.
    pub(crate) fn adopt(&mut self, view: &Window, size: Size) -> Result<(), String> {
        self.view = view.handle();
        let hwnd = self.hwnd;
        // SAFETY: a window handle; a gone one makes the call return false.
        if unsafe { IsWindow(hwnd) } == 0 {
            return Err("the program's window closed".into());
        }
        // SAFETY: as above; a maximised window comes back to a plain size before it moves.
        if unsafe { IsZoomed(hwnd) } != 0 {
            // SAFETY: as above.
            unsafe { ShowWindow(hwnd, SW_RESTORE) };
        }
        // SAFETY: as above; reads its styles.
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
        // SAFETY: as above.
        let extended = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        let frame = (WS_POPUP | WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        let style = (style & !frame) | (WS_CHILD | WS_VISIBLE) as isize;
        let edges =
            (WS_EX_APPWINDOW | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE | WS_EX_DLGMODALFRAME) as isize;
        // SAFETY: plain style bits on a window, no window procedure or pointer.
        unsafe { SetWindowLongPtrW(hwnd, GWL_STYLE, style) };
        // SAFETY: as above.
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, extended & !edges) };
        // SAFETY: two window handles; the view is this process's, open while `view` lives.
        if unsafe { SetParent(hwnd, self.view) }.is_null() {
            return Err("could not take the program's window in".into());
        }
        self.place(size, SWP_FRAMECHANGED | SWP_SHOWWINDOW);
        Ok(())
    }

    fn place(&self, size: Size, flags: u32) {
        let span = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
        per_monitor(|| {
            // SAFETY: a window handle; a gone one makes the call fail.
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    span(size.width),
                    span(size.height),
                    SWP_NOZORDER | SWP_NOACTIVATE | flags,
                )
            };
        });
    }

    /// Whether the window is still there.
    pub(crate) fn alive(&self) -> bool {
        // SAFETY: only asks whether the handle names a window.
        unsafe { IsWindow(self.hwnd) != 0 }
    }

    /// Keeps the window at the view's top left corner and `size`, should the program move it.
    pub(crate) fn fit(&self, size: Size) {
        let now = per_monitor(|| {
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            let mut origin = POINT { x: 0, y: 0 };
            // SAFETY: a window handle and a rectangle to fill; a gone window makes it fail.
            let placed = unsafe { GetWindowRect(self.hwnd, &mut rect) } != 0;
            // SAFETY: the view, this process's window, and a point to fill.
            let read = placed && unsafe { ClientToScreen(self.view, &mut origin) } != 0;
            read.then(|| (rect.left - origin.x, rect.top - origin.y, rect.right - rect.left, rect.bottom - rect.top))
        });
        let span = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
        if now.is_some_and(|now| now != (0, 0, span(size.width), span(size.height))) {
            self.place(size, 0);
        }
    }

    /// Nothing to serve here: the program's window takes its input through its own thread.
    pub(crate) fn serve(&self) {}

    /// Asks the program to close its window, as its own close button would.
    pub(crate) fn close(&self) {
        // SAFETY: a message without pointers; a gone window makes the call fail.
        unsafe { PostMessageW(self.hwnd, WM_CLOSE, 0, 0) };
    }
}
