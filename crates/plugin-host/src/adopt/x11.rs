// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Adopting a program's window on Linux (X11): the program's top-level window is reparented
//! into the embedded view, out of the window manager's frame, and given the keyboard when the
//! pointer comes in (no window manager hands it out inside another program's window).

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask,
    InputFocus, MapState, Window as XWindow,
};
use x11rb::rust_connection::RustConnection;

use crate::window::{Size, Window};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        _NET_WM_PID,
        _NET_WM_NAME,
        UTF8_STRING,
    }
}

/// The program's main window, once found.
pub(crate) struct Program {
    connection: RustConnection,
    atoms: Atoms,
    window: XWindow,
    view: XWindow,
}

/// The smallest window taken for a program's main one: a splash or a tooltip is smaller.
const SMALLEST: u16 = 200;

impl Program {
    /// The top-level window of process `pid`: shown, not a dialog (no `WM_TRANSIENT_FOR`), not
    /// a menu (no override-redirect), the largest if there are several.
    pub(crate) fn find(pid: u32, title: Option<&str>) -> Option<Self> {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let atoms = Atoms::new(&connection).ok()?.reply().ok()?;
        let mut best: Option<(u32, XWindow)> = None;
        let mut look = vec![(root, 0u8)];
        while let Some((window, depth)) = look.pop() {
            let Ok(Ok(tree)) = connection.query_tree(window).map(|c| c.reply()) else { continue };
            for child in tree.children {
                if depth < 2 {
                    look.push((child, depth + 1));
                }
                if let Some(area) = main_window_area(&connection, &atoms, child, pid, title)
                    && best.is_none_or(|(most, _)| area > most)
                {
                    best = Some((area, child));
                }
            }
        }
        let (_, window) = best?;
        Some(Self { connection, atoms, window, view: 0 })
    }

    /// Puts the window in `view`, at its top left corner and `size`.
    pub(crate) fn adopt(&mut self, view: &Window, size: Size) -> Result<(), String> {
        let failed = |e: x11rb::errors::ConnectionError| format!("could not take the program's window in: {e}");
        self.view = u32::try_from(view.handle() as usize).map_err(|_| "no view to take the program's window in")?;
        let enter = ChangeWindowAttributesAux::new().event_mask(EventMask::ENTER_WINDOW);
        self.connection.change_window_attributes(self.view, &enter).map_err(failed)?;
        // Out of the window manager's hands: unmapped, then into the view, then shown.
        self.connection.unmap_window(self.window).map_err(failed)?;
        self.connection.reparent_window(self.window, self.view, 0, 0).map_err(failed)?;
        self.connection.map_window(self.window).map_err(failed)?;
        self.connection.flush().map_err(failed)?;
        self.fit(size);
        Ok(())
    }

    /// Whether the window is still there.
    pub(crate) fn alive(&self) -> bool {
        self.connection.get_geometry(self.window).ok().and_then(|c| c.reply().ok()).is_some()
    }

    /// Keeps the window at the view's top left corner and `size`, should the program move it.
    pub(crate) fn fit(&self, size: Size) {
        let Ok(Ok(now)) = self.connection.get_geometry(self.window).map(|c| c.reply()) else { return };
        let (width, height) = (u32::from(now.width), u32::from(now.height));
        if (now.x, now.y, width, height) == (0, 0, size.width, size.height) {
            return;
        }
        let aux = ConfigureWindowAux::new().x(0).y(0).width(size.width).height(size.height);
        let _ = self.connection.configure_window(self.window, &aux);
        let _ = self.connection.flush();
    }

    /// The pointer came into the view: the keyboard goes to the program, as a click would give
    /// it to a window of its own.
    pub(crate) fn serve(&self) {
        while let Ok(Some(event)) = self.connection.poll_for_event() {
            if let Event::EnterNotify(enter) = event
                && enter.event == self.view
            {
                let _ = self.connection.set_input_focus(InputFocus::PARENT, self.window, x11rb::CURRENT_TIME);
                let _ = self.connection.flush();
            }
        }
    }

    /// Asks the program to close its window, as its own close button would.
    pub(crate) fn close(&self) {
        let message = ClientMessageEvent::new(
            32,
            self.window,
            self.atoms.WM_PROTOCOLS,
            [self.atoms.WM_DELETE_WINDOW, x11rb::CURRENT_TIME, 0, 0, 0],
        );
        let _ = self.connection.send_event(false, self.window, EventMask::NO_EVENT, message);
        let _ = self.connection.flush();
    }
}

/// The area of `window` when it is a main window of process `pid`.
fn main_window_area(
    connection: &RustConnection,
    atoms: &Atoms,
    window: XWindow,
    pid: u32,
    title: Option<&str>,
) -> Option<u32> {
    let owner = connection
        .get_property(false, window, atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()?;
    if owner != pid {
        return None;
    }
    let attributes = connection.get_window_attributes(window).ok()?.reply().ok()?;
    if attributes.override_redirect || attributes.map_state != MapState::VIEWABLE {
        return None;
    }
    let transient = connection
        .get_property(false, window, AtomEnum::WM_TRANSIENT_FOR, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    if transient.value_len > 0 {
        return None;
    }
    let name = |property: u32, kind: u32| {
        let reply = connection.get_property(false, window, property, kind, 0, 256).ok()?.reply().ok()?;
        Some(String::from_utf8_lossy(&reply.value).into_owned())
    };
    let named = name(atoms._NET_WM_NAME, atoms.UTF8_STRING)
        .filter(|n| !n.is_empty())
        .or_else(|| name(AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
        .unwrap_or_default();
    if !super::titled(&named, title) {
        return None;
    }
    let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
    (geometry.width >= SMALLEST && geometry.height >= SMALLEST)
        .then(|| u32::from(geometry.width) * u32::from(geometry.height))
}
