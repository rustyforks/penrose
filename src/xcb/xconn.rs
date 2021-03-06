/*!
 *  API wrapper for talking to the X server using XCB
 *
 *  The crate used by penrose for talking to the X server is rust-xcb, which
 *  is a set of bindings for the C level XCB library that are autogenerated
 *  from an XML spec. The XML files can be found
 *  [here](https://github.com/rtbo/rust-xcb/tree/master/xml) and are useful
 *  as reference for how the API works. Sections have been converted and added
 *  to the documentation of the method calls and enums present in this module.
 *
 *  [EWMH](https://specifications.freedesktop.org/wm-spec/wm-spec-1.3.html)
 *  [Xlib manual](https://tronche.com/gui/x/xlib/)
 */
use crate::{
    core::{
        bindings::{KeyBindings, MouseBindings},
        data_types::{Point, PropVal, Region, WinAttr, WinConfig, WinId, WinType},
        manager::WindowManager,
        screen::Screen,
        xconnection::{
            Atom, XConn, XEvent, AUTO_FLOAT_WINDOW_TYPES, EWMH_SUPPORTED_ATOMS,
            UNMANAGED_WINDOW_TYPES,
        },
    },
    xcb::{Api, XcbApi},
    Result,
};

use std::{collections::HashMap, str::FromStr};

const WM_NAME: &str = "penrose";

/**
 * Handles communication with an X server via the XCB library.
 *
 * XcbConnection is a minimal implementation that does not make use of the full asyc capabilities
 * of the underlying C XCB library.
 **/
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct XcbConnection {
    api: Api,
    check_win: WinId,
    auto_float_types: Vec<u32>,
    dont_manage_types: Vec<u32>,
}

impl XcbConnection {
    /// Establish a new connection to the running X server. Fails if unable to connect
    pub fn new() -> Result<Self> {
        let api = Api::new()?;
        let auto_float_types: Vec<u32> = AUTO_FLOAT_WINDOW_TYPES
            .iter()
            .map(|a| api.known_atom(*a))
            .collect();
        let dont_manage_types: Vec<u32> = UNMANAGED_WINDOW_TYPES
            .iter()
            .map(|a| api.known_atom(*a))
            .collect();

        api.set_randr_notify_mask()?;
        let check_win = api.create_window(WinType::CheckWin, Region::new(0, 0, 1, 1), false)?;

        Ok(Self {
            api,
            check_win,
            auto_float_types,
            dont_manage_types,
        })
    }

    fn window_has_type_in(&self, id: WinId, win_types: &[u32]) -> bool {
        if let Ok(atom) = self.api.get_atom_prop(id, Atom::NetWmWindowType) {
            return win_types.contains(&atom);
        }
        false
    }

    /// Get a handle on the underlying [XCB Connection][::xcb::Connection] used by [Api]
    /// to communicate with the X server.
    pub fn xcb_connection(&self) -> &xcb::Connection {
        &self.api.conn()
    }

    /// Get a handle on the underlying [Api] to communicate with the X server.
    pub fn api(&self) -> &Api {
        &self.api
    }

    /// Get a mutable handle on the underlying [Api] to communicate with the X server.
    pub fn api_mut(&mut self) -> &mut Api {
        &mut self.api
    }

    /// The current interned [Atom] values known to the underlying [Api] connection
    pub fn known_atoms(&self) -> &HashMap<Atom, u32> {
        &self.api.known_atoms()
    }
}

impl WindowManager<XcbConnection> {
    /// Get a handle on the underlying XCB Connection used by [Api] to communicate with the X
    /// server.
    pub fn xcb_connection(&self) -> &xcb::Connection {
        &self.conn().xcb_connection()
    }

    /// The current interned [Atom] values known to the underlying [XcbConnection]
    pub fn known_atoms(&self) -> &HashMap<Atom, u32> {
        &self.conn().known_atoms()
    }
}

impl XConn for XcbConnection {
    #[cfg(feature = "serde")]
    fn hydrate(&mut self) -> Result<()> {
        Ok(self.api.hydrate()?)
    }

    fn flush(&self) -> bool {
        self.api.flush()
    }

    fn wait_for_event(&self) -> Result<XEvent> {
        Ok(self.api.wait_for_event()?)
    }

    fn current_outputs(&self) -> Vec<Screen> {
        match self.api.current_screens() {
            Ok(screens) => screens,
            Err(e) => panic!("{}", e),
        }
    }

    fn cursor_position(&self) -> Point {
        self.api.cursor_position()
    }

    fn position_window(&self, id: WinId, reg: Region, border: u32, stack_above: bool) {
        let mut data = vec![WinConfig::Position(reg), WinConfig::BorderPx(border)];
        if stack_above {
            data.push(WinConfig::StackAbove);
        }
        self.api.configure_window(id, &data)
    }

    fn raise_window(&self, id: WinId) {
        self.api.configure_window(id, &[WinConfig::StackAbove])
    }

    fn mark_new_window(&self, id: WinId) {
        let data = &[WinAttr::ClientEventMask];
        self.api.set_window_attributes(id, data)
    }

    fn map_window(&self, id: WinId) {
        self.api.map_window(id);
    }

    fn unmap_window(&self, id: WinId) {
        self.api.unmap_window(id);
    }

    fn send_client_event(&self, id: WinId, atom_name: &str) -> Result<()> {
        Ok(self.api.send_client_event(id, atom_name)?)
    }

    fn focused_client(&self) -> WinId {
        self.api.focused_client().unwrap_or(0)
    }

    fn focus_client(&self, id: WinId) {
        self.api.mark_focused_window(id);
    }

    fn set_client_border_color(&self, id: WinId, color: u32) {
        let data = &[WinAttr::BorderColor(color)];
        self.api.set_window_attributes(id, data);
    }

    fn toggle_client_fullscreen(&self, id: WinId, client_is_fullscreen: bool) {
        let data = if client_is_fullscreen {
            0
        } else {
            self.api.known_atom(Atom::NetWmStateFullscreen)
        };

        self.api
            .replace_prop(id, Atom::NetWmState, PropVal::Atom(&[data]));
    }

    fn grab_keys(&self, key_bindings: &KeyBindings<Self>, mouse_bindings: &MouseBindings<Self>) {
        self.api.grab_keys(&key_bindings.keys().collect::<Vec<_>>());
        self.api.grab_mouse_buttons(
            &mouse_bindings
                .keys()
                .map(|(_, state)| state)
                .collect::<Vec<_>>(),
        );
        let data = &[WinAttr::RootEventMask];
        self.api.set_window_attributes(self.api.root(), data);
        self.flush();
    }

    fn set_wm_properties(&self, workspaces: &[&str]) {
        let root = self.api.root();
        for &win in &[self.check_win, root] {
            self.api.replace_prop(
                win,
                Atom::NetSupportingWmCheck,
                PropVal::Window(&[self.check_win]),
            );
            let val = PropVal::Str(WM_NAME);
            self.api.replace_prop(win, Atom::WmName, val);
        }

        // EWMH support
        let supported = EWMH_SUPPORTED_ATOMS
            .iter()
            .map(|a| self.api.known_atom(*a))
            .collect::<Vec<u32>>();
        let prop = PropVal::Atom(&supported);

        self.api.replace_prop(root, Atom::NetSupported, prop);
        self.update_desktops(workspaces);
        self.api.delete_prop(root, Atom::NetClientList);
    }

    fn update_desktops(&self, workspaces: &[&str]) {
        let root = self.api.root();
        self.api.replace_prop(
            root,
            Atom::NetNumberOfDesktops,
            PropVal::Cardinal(&[workspaces.len() as u32]),
        );
        self.api.replace_prop(
            root,
            Atom::NetDesktopNames,
            PropVal::Str(&workspaces.join("\0")),
        );
    }

    fn update_known_clients(&self, clients: &[WinId]) {
        self.api.replace_prop(
            self.api.root(),
            Atom::NetClientList,
            PropVal::Window(clients),
        );
        self.api.replace_prop(
            self.api.root(),
            Atom::NetClientListStacking,
            PropVal::Window(clients),
        );
    }

    fn set_current_workspace(&self, wix: usize) {
        self.api.replace_prop(
            self.api.root(),
            Atom::NetCurrentDesktop,
            PropVal::Cardinal(&[wix as u32]),
        );
    }

    fn set_root_window_name(&self, root_name: &str) {
        self.api
            .replace_prop(self.api.root(), Atom::WmName, PropVal::Str(root_name));
    }

    fn set_client_workspace(&self, id: WinId, workspace: usize) {
        self.api.replace_prop(
            id,
            Atom::NetWmDesktop,
            PropVal::Cardinal(&[workspace as u32]),
        );
    }

    fn window_should_float(&self, id: WinId, floating_classes: &[&str]) -> bool {
        if let Ok(s) = self.str_prop(id, Atom::WmClass.as_ref()) {
            if s.split('\0').any(|c| floating_classes.contains(&c)) {
                return true;
            }
        }
        self.window_has_type_in(id, &self.auto_float_types)
    }

    fn is_managed_window(&self, id: WinId) -> bool {
        !self.window_has_type_in(id, &self.dont_manage_types)
    }

    fn window_geometry(&self, id: WinId) -> Result<Region> {
        Ok(self.api.window_geometry(id)?)
    }

    fn warp_cursor(&self, win_id: Option<WinId>, screen: &Screen) {
        let (x, y, id) = match win_id {
            Some(id) => {
                let (_, _, w, h) = match self.window_geometry(id) {
                    Ok(region) => region.values(),
                    Err(e) => {
                        error!("error fetching window details while warping cursor: {}", e);
                        return;
                    }
                };
                ((w / 2), (h / 2), id)
            }
            None => {
                let (x, y, w, h) = screen.region(true).values();
                ((x + w / 2), (y + h / 2), self.api.root())
            }
        };

        self.api.warp_cursor(id, x as usize, y as usize);
    }

    fn query_for_active_windows(&self) -> Vec<WinId> {
        match self.api.current_clients() {
            Err(_) => Vec::new(),
            Ok(ids) => ids
                .iter()
                .filter(|&id| !self.window_has_type_in(*id, &self.dont_manage_types))
                .cloned()
                .collect(),
        }
    }

    fn str_prop(&self, id: u32, name: &str) -> Result<String> {
        Ok(self.api.get_str_prop(id, name)?)
    }

    fn atom_prop(&self, id: u32, name: &str) -> Result<u32> {
        Ok(self.api.get_atom_prop(id, Atom::from_str(name)?)?)
    }

    fn intern_atom(&self, atom: &str) -> Result<u32> {
        Ok(self.api.atom(atom)?)
    }

    // - Release all of the keybindings we are holding on to
    // - destroy the check window
    // - mark ourselves as no longer being the active root window
    fn cleanup(&self) {
        self.api.ungrab_keys();
        self.api.ungrab_mouse_buttons();
        self.api.destroy_window(self.check_win);
        self.api.delete_prop(self.api.root(), Atom::NetActiveWindow);
    }
}
