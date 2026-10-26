//! The Wayland backend. Under X11 the menu is an override-redirect
//! window that grabs the keyboard, which Wayland doesn't allow: under
//! Xwayland the grab "succeeds" but keys keep going to whatever Wayland
//! window has focus, so Escape and typing never reach the menu. Here
//! the menu is instead a layer-shell overlay with exclusive keyboard
//! focus, so it gets every key and every click on it.
//!
//! Drawing is unchanged: Xft still renders into the menu's pixmap (on
//! Xwayland, offscreen), and `present` copies the finished frame into
//! a Wayland buffer instead of an X window. Keys and clicks are handed
//! to the same handlers the X11 event loop uses.
use std::{
    io::{BufRead, BufReader, Write},
    num::NonZeroU32,
    os::unix::{io::AsRawFd, net::UnixStream},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers, RepeatInfo},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, EventQueue, QueueHandle,
};
use x11::xlib::{Button1, Button2, Button3, Button4, Button5, ControlMask, Mod1Mask, Mod4Mask, ShiftMask};

use crate::result::*;

/// Set once the menu is running on Wayland, so the X11-only steps
/// (grabbing the keyboard, mapping a window) know to stand aside.
pub static ACTIVE: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The connection, once `main` has made one
    pub static WL: std::cell::RefCell<Option<Wayland>> = std::cell::RefCell::new(None);
}

pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Whether to use Wayland: when there's a Wayland display, unless
/// `FLEXIMENU_BACKEND=x11` asks for the X11 window.
pub fn wanted() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").map_or(false, |d| !d.is_empty())
        && std::env::var("FLEXIMENU_BACKEND").map_or(true, |b| b != "x11")
}

/// What the menu should react to, already in X11's terms.
#[derive(Clone, Debug)]
pub enum Input {
    /// a keysym, the text it types, and X modifier masks
    Key { sym: u32, text: String, state: u32 },
    /// X button 1-5 at a point, with X modifier masks
    Button { button: u32, x: i32, y: i32, state: u32 },
    Motion { x: i32, y: i32 },
}

struct State {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    /// the size of the last configure, once there's been one
    configured: Option<(u32, u32)>,
    closed: bool,
    mods: Modifiers,
    repeat: Option<(Duration, Duration)>,
    /// the key held down, when it was pressed, and when it next repeats
    held: Option<(Input, u32, Instant)>,
    input: Vec<Input>,
}

pub struct Wayland {
    queue: EventQueue<State>,
    state: State,
}

fn mask(m: &Modifiers) -> u32 {
    let mut s = 0;
    if m.shift {
        s |= ShiftMask;
    }
    if m.ctrl {
        s |= ControlMask;
    }
    if m.alt {
        s |= Mod1Mask;
    }
    if m.logo {
        s |= Mod4Mask;
    }
    s
}

impl Wayland {
    /// Connect and open the menu's surface over the whole of the area
    /// windows can use, to learn how big that is.
    pub fn connect() -> CompResult<Self> {
        let err = |e: &dyn std::fmt::Display| Die::Stderr(format!("wayland: {}", e));
        let conn = Connection::connect_to_env().map_err(|e| err(&e))?;
        let (globals, queue) = registry_queue_init(&conn).map_err(|e| err(&e))?;
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh).map_err(|e| err(&e))?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|e| err(&format!("no layer shell ({}); set FLEXIMENU_BACKEND=x11", e)))?;
        let shm = Shm::bind(&globals, &qh).map_err(|e| err(&e))?;
        let surface = compositor.create_surface(&qh);
        let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("fleximenu"), None);
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.set_size(0, 0);
        layer.commit();
        let pool = SlotPool::new(4096, &shm).map_err(|e| err(&e))?;
        let state = State {
            registry: RegistryState::new(&globals),
            seats: SeatState::new(&globals, &qh),
            outputs: OutputState::new(&globals, &qh),
            shm,
            pool,
            layer,
            keyboard: None,
            pointer: None,
            configured: None,
            closed: false,
            mods: Modifiers::default(),
            repeat: Some((Duration::from_millis(600), Duration::from_millis(40))),
            held: None,
            input: vec![],
        };
        let mut wl = Self { queue, state };
        wl.wait_configure()?;
        ACTIVE.store(true, Ordering::Relaxed);
        Ok(wl)
    }

    fn wait_configure(&mut self) -> CompResult<(u32, u32)> {
        self.state.configured = None;
        while self.state.configured.is_none() {
            if self.state.closed {
                return Die::stderr("wayland: the menu's surface was closed".to_owned());
            }
            self.queue
                .blocking_dispatch(&mut self.state)
                .map_err(|e| Die::Stderr(format!("wayland: {}", e)))?;
        }
        Ok(self.state.configured.unwrap())
    }

    /// The size of the area the menu is placed in: the output, less any
    /// bar's exclusive zone.
    pub fn area(&self) -> (i32, i32) {
        let (w, h) = self.state.configured.unwrap_or((0, 0));
        (w as i32, h as i32)
    }

    /// Move the menu to `x`, `y` in `area()` and make it `w`x`h`.
    /// The first configure is already acked, so the menu can draw at the
    /// size it asks for straight away; river won't configure again until
    /// the surface has a buffer, so waiting for that would hang.
    pub fn place(&mut self, x: i32, y: i32, w: i32, h: i32) -> CompResult<()> {
        let layer = &self.state.layer;
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(y.max(0), 0, 0, x.max(0));
        layer.set_size(w.max(1) as u32, h.max(1) as u32);
        layer.commit();
        self.queue
            .roundtrip(&mut self.state)
            .map(|_| ())
            .map_err(|e| Die::Stderr(format!("wayland: {}", e)))
    }

    /// Show a `w`x`h` frame of XRGB8888 pixels (X's 32-bit ZPixmap
    /// layout on little-endian machines), `stride` bytes a row.
    pub fn present(&mut self, data: &[u8], w: i32, h: i32, stride: usize) {
        let s = &mut self.state;
        let (buffer, canvas) = match s.pool.create_buffer(w, h, w * 4, wl_shm::Format::Xrgb8888) {
            Ok(b) => b,
            Err(_) => return,
        };
        let row = (w * 4) as usize;
        for y in 0..h as usize {
            canvas[y * row..(y + 1) * row].copy_from_slice(&data[y * stride..y * stride + row]);
        }
        let surface = s.layer.wl_surface();
        surface.damage_buffer(0, 0, w, h);
        let _ = buffer.attach_to(surface);
        s.layer.commit();
        let _ = self.queue.flush();
    }

    /// Wait for input, returning what arrived (including key repeats).
    pub fn next(&mut self) -> CompResult<Vec<Input>> {
        loop {
            if self.state.closed {
                return Die::stderr("".to_owned());
            }
            let ready = std::mem::take(&mut self.state.input);
            if !ready.is_empty() {
                return Ok(ready);
            }
            if let Some((input, _, at)) = &mut self.state.held {
                if Instant::now() >= *at {
                    let input = input.clone();
                    *at = Instant::now() + self.state.repeat.map_or(Duration::from_millis(40), |r| r.1);
                    return Ok(vec![input]);
                }
            }
            let _ = self.queue.flush();
            let timeout = self
                .state
                .held
                .as_ref()
                .map_or(-1, |(_, _, at)| at.saturating_duration_since(Instant::now()).as_millis() as i32);
            let read = match self.queue.prepare_read() {
                Some(guard) => guard,
                None => {
                    self.dispatch()?;
                    continue;
                }
            };
            let mut fds = [libc::pollfd {
                fd: read.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }];
            unsafe { libc::poll(fds.as_mut_ptr(), 1, timeout) };
            if fds[0].revents != 0 {
                let _ = read.read();
            }
            self.dispatch()?;
        }
    }

    fn dispatch(&mut self) -> CompResult<()> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map(|_| ())
            .map_err(|e| Die::Stderr(format!("wayland: {}", e)))
    }
}

/// The pointer's position in the menu's area, from flexiflow-river, for
/// `--mouse`: Wayland itself doesn't tell a client where the pointer is
/// until it's over one of its surfaces.
pub fn pointer_position() -> Option<(i32, i32)> {
    let dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let display = std::env::var("WAYLAND_DISPLAY").ok()?;
    let mut s = UnixStream::connect(format!("{}/flexiflow-river.{}.sock", dir, display)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(200))).ok()?;
    s.write_all(b"pointer\n").ok()?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).ok()?;
    let mut it = line.split_whitespace().map(|n| n.parse::<i32>());
    match (it.next(), it.next()) {
        (Some(Ok(x)), Some(Ok(y))) => Some((x, y)),
        _ => None,
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.configured = Some(configure.new_size);
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
        if cap == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl State {
    fn key(&mut self, event: &KeyEvent) -> Input {
        Input::Key {
            sym: event.keysym.raw(),
            text: event.utf8.clone().unwrap_or_default(),
            state: mask(&self.mods),
        }
    }
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {
        self.held = None;
    }

    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        let input = self.key(&event);
        self.input.push(input.clone());
        // modifiers alone don't repeat
        let modifier = (0xffe1..=0xffee).contains(&event.keysym.raw());
        self.held = match self.repeat {
            Some((delay, _)) if !modifier => Some((input, event.raw_code, Instant::now() + delay)),
            _ => None,
        };
    }

    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}

    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        if self.held.as_ref().map_or(false, |(_, code, _)| *code == event.raw_code) {
            self.held = None;
        }
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.mods = modifiers;
    }

    fn update_repeat_info(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, info: RepeatInfo) {
        self.repeat = match info {
            RepeatInfo::Repeat { rate, delay } => Some((
                Duration::from_millis(delay as u64),
                Duration::from_millis(1000 / NonZeroU32::get(rate) as u64),
            )),
            RepeatInfo::Disable => None,
        };
    }
}

impl PointerHandler for State {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        let state = mask(&self.mods);
        for e in events {
            let (x, y) = (e.position.0 as i32, e.position.1 as i32);
            match e.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.input.push(Input::Motion { x, y })
                }
                // BTN_LEFT, BTN_RIGHT, BTN_MIDDLE as X's buttons 1, 3, 2
                PointerEventKind::Press { button, .. } => {
                    let button = match button {
                        0x110 => Button1,
                        0x112 => Button2,
                        0x111 => Button3,
                        _ => continue,
                    };
                    self.input.push(Input::Button { button, x, y, state });
                }
                PointerEventKind::Axis { vertical, .. } => {
                    let dir = if vertical.discrete != 0 {
                        vertical.discrete as f64
                    } else {
                        vertical.absolute
                    };
                    if dir != 0.0 {
                        let button = if dir < 0.0 { Button4 } else { Button5 };
                        self.input.push(Input::Button { button, x, y, state });
                    }
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_keyboard!(State);
delegate_pointer!(State);
delegate_layer!(State);
delegate_registry!(State);
