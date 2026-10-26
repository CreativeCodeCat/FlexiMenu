use arboard::Clipboard;
use libc::{c_char, c_int, c_ulong, iscntrl};
use regex::Regex;
use std::mem::MaybeUninit;
use unicode_segmentation::UnicodeSegmentation;
use x11::xlib::{
    Button1, Button2, Button3, Button4, Button5, ButtonPress, ControlMask, DestroyNotify, Expose,
    False, FocusIn, KeyPress, Mod1Mask, MotionNotify, SelectionNotify, ShiftMask, VisibilityNotify,
    VisibilityUnobscured, XButtonEvent, XEvent, XFilterEvent, XInternAtom, XKeyEvent, XLookupBoth,
    XLookupChars, XLookupKeySym, XMotionEvent, XNextEvent, XRaiseWindow, XmbLookupString,
};

use crate::drw::Drw;
use crate::item::{Click, Partition};
use crate::result::*;
use crate::util::grabfocus;

#[allow(non_upper_case_globals)]
impl Drw {
    pub fn run(&mut self) -> CompResult<()> {
        if crate::wayland::active() {
            return self.run_wayland();
        }
        unsafe {
            let utf8 = XInternAtom(self.dpy, "UTF8_STRING\0".as_ptr() as *mut c_char, False);
            let mut ev: XEvent = MaybeUninit::uninit().assume_init();
            while XNextEvent(self.dpy, &mut ev) == 0 {
                if XFilterEvent(&mut ev, self.pseudo_globals.win) != 0 {
                    continue;
                }

                match ev.type_ {
                    DestroyNotify => {
                        if ev.destroy_window.window != self.pseudo_globals.win {
                            break;
                        }
                    }
                    Expose => {
                        if ev.expose.count == 0 {
                            self.map(self.pseudo_globals.win, 0, 0, self.w, self.h);
                        }
                    }
                    FocusIn => {
                        /* regrab focus from parent window */
                        grabfocus(self)?;
                    }
                    KeyPress => match self.keypress(ev.key) {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(err) => return Err(err),
                    },
                    ButtonPress => match self.buttonpress(ev.button) {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(err) => return Err(err),
                    },
                    MotionNotify => self.motion(ev.motion)?,
                    SelectionNotify => {
                        if ev.selection.property == utf8 {
                            self.paste()?;
                        }
                    }
                    VisibilityNotify => {
                        if ev.visibility.state != VisibilityUnobscured {
                            XRaiseWindow(self.dpy, self.pseudo_globals.win);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn keypress(&mut self, mut ev: XKeyEvent) -> CompResult<bool> {
        // bool - should exit?
        unsafe {
            let buf: [u8; 32] = [0; 32];
            let mut __ksym = MaybeUninit::<c_ulong>::uninit();
            let mut status = MaybeUninit::<c_int>::uninit();
            let len = XmbLookupString(
                self.pseudo_globals.xic,
                &mut ev,
                buf.as_ptr() as *mut i8,
                buf.len() as i32,
                __ksym.as_mut_ptr(),
                status.as_mut_ptr(),
            );
            let ksym = __ksym.assume_init() as u32;
            match status.assume_init() {
                XLookupChars => {
                    if iscntrl(*(buf.as_ptr() as *mut i32)) == 0 {
                        self.keyprocess(ksym, buf, len, ev.state)?;
                    }
                }
                XLookupKeySym | XLookupBoth => {}
                _ => return Ok(false), /* XLookupNone, XBufferOverflow */
            }
            self.key(ksym, buf, len, ev.state)
        }
    }

    /// The Wayland event loop: the same keys, clicks and pointer motion
    /// as `run`, from the layer surface in wayland.rs
    fn run_wayland(&mut self) -> CompResult<()> {
        use crate::wayland::{Input, WL};
        loop {
            let inputs = WL.with(|wl| wl.borrow_mut().as_mut().unwrap().next())?;
            for input in inputs {
                let exit = match input {
                    Input::Key { sym, text, state } => {
                        let mut buf = [0u8; 32];
                        let bytes = text.as_bytes();
                        let len = bytes.len().min(buf.len());
                        buf[..len].copy_from_slice(&bytes[..len]);
                        self.key(sym, buf, len as i32, state)?
                    }
                    Input::Button { button, x, y, state } => self.button(button, x, y, state)?,
                    Input::Motion { x, y } => {
                        self.motion_at(x, y)?;
                        false
                    }
                };
                if exit {
                    return Ok(());
                }
            }
        }
    }

    /// A key, after X's or xkbcommon's lookup: Emacs-style Ctrl and Alt
    /// bindings first, then everything else
    fn key(&mut self, mut ksym: u32, buf: [u8; 32], len: i32, mut state: u32) -> CompResult<bool> {
        // bool - should exit?
        use x11::keysym::*;
        {
            const control: bool = true;
            const mod1: bool = false;
            if (state & ControlMask) != 0 || (state & Mod1Mask) != 0 {
                match (ksym, (state & ControlMask) != 0) {
                    (XK_a, control) | (XK_g, mod1) => ksym = XK_Home,
                    (XK_b, control) => ksym = XK_Left,
                    (XK_c, control) => ksym = XK_Escape,
                    (XK_d, control) => ksym = XK_Delete,
                    (XK_e, control) | (XK_G, mod1) => ksym = XK_End,
                    (XK_f, control) => ksym = XK_Right,
                    (XK_g, control) | (XK_bracketleft, control) => ksym = XK_Escape,
                    (XK_h, control) => ksym = XK_BackSpace,
                    (XK_i, control) => ksym = XK_Tab,
                    (XK_j, mod1) => ksym = XK_Next,
                    (XK_k, mod1) => ksym = XK_Prior,
                    (XK_n, control) | (XK_l, mod1) => ksym = XK_Down,
                    (XK_p, control) | (XK_h, mod1) => ksym = XK_Up,
                    (XK_j, control) | (XK_J, control) | (XK_m, control) | (XK_M, control) => {
                        ksym = XK_Return;
                        state &= !ControlMask;
                    }
                    (XK_k, control) => {
                        // delete all to the left
                        self.input = self
                            .input
                            .graphemes(true)
                            .take(self.pseudo_globals.cursor)
                            .collect::<String>();
                        return self.draw().map(|_| false);
                    }
                    (XK_u, control) => {
                        // delete all to the right
                        self.input = self
                            .input
                            .graphemes(true)
                            .skip(self.pseudo_globals.cursor)
                            .collect::<String>();
                        self.pseudo_globals.cursor = 0;
                        return self.draw().map(|_| false);
                    }
                    (XK_w, control) | (XK_BackSpace, control) => {
                        // Delete word to the left
                        let mut state = 0;
                        let mut found = 0;
                        self.input = self
                            .input
                            .grapheme_indices(true)
                            .rev()
                            .filter_map(|(i, c)| {
                                if state == 0 && i < self.pseudo_globals.cursor {
                                    state = 1; // searching for cursor
                                }
                                if state == 1 && c != " " {
                                    state = 2; // looking for previous word
                                }
                                if state == 2 && c == " " {
                                    state = 3; // skipping past next word
                                }
                                if state == 0 || state == 4 {
                                    Some(c)
                                } else if state == 3 {
                                    found = i + 1;
                                    state = 4;
                                    Some(c)
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<&str>>()
                            .into_iter()
                            .rev()
                            .collect::<String>();
                        self.pseudo_globals.cursor = found;
                        return self.draw().map(|_| false);
                    }
                    (XK_Delete, control) => {
                        // Delete word to the right
                        let mut state = 0;
                        self.input = self
                            .input
                            .grapheme_indices(true)
                            .filter_map(|(i, c)| {
                                if state == 0 && i >= self.pseudo_globals.cursor {
                                    state = 1; // searching for cursor
                                }
                                if state == 1 && c != " " {
                                    state = 2; // looking for next word
                                }
                                if state == 2 && c == " " {
                                    state = 3; // skipping past next word
                                }
                                if state == 0 || state == 4 {
                                    Some(c)
                                } else if state == 3 {
                                    state = 4;
                                    Some(c)
                                } else {
                                    None
                                }
                            })
                            .collect::<String>();
                        return self.draw().map(|_| false);
                    }
                    (XK_y, control) | (XK_Y, control) => {
                        // paste selection
                        return self.paste().map(|_| false);
                    }
                    (XK_Left, control) | (XK_b, mod1) => {
                        // skip to word boundary on left
                        self.pseudo_globals.cursor = self
                            .input
                            .grapheme_indices(true)
                            .rev()
                            .skip(self.input.graphemes(true).count() - self.pseudo_globals.cursor)
                            .skip_while(|(_, c)| *c == " ") // find last word
                            .skip_while(|(_, c)| *c != " ") // skip past it
                            .next()
                            .map(|(i, _)| i + 1)
                            .unwrap_or(0);
                        return self.draw().map(|_| false);
                    }
                    (XK_Right, control) | (XK_f, mod1) => {
                        // skip to word boundary on right
                        self.pseudo_globals.cursor = self
                            .input
                            .grapheme_indices(true)
                            .skip(self.pseudo_globals.cursor + 1)
                            .skip_while(|(_, c)| *c == " ") // find next word
                            .skip_while(|(_, c)| *c != " ") // skip past it
                            .next()
                            .map(|(i, _)| i)
                            .unwrap_or(self.input.graphemes(true).count());
                        return self.draw().map(|_| false);
                    }
                    (XK_Return, control) | (XK_KP_Enter, control) => {} // pass through
                    _ => return Ok(false),
                }
            }
            self.keyprocess(ksym, buf, len, state)
        }
    }

    /// The item or arrow at a point in the menu, as last drawn
    fn click_at(&self, x: c_int, y: c_int) -> Option<Click> {
        self.items
            .as_ref()
            .unwrap()
            .hitboxes
            .iter()
            .find(|hitbox| hitbox.contains(x, y))
            .map(|hitbox| hitbox.click)
    }

    fn buttonpress(&mut self, ev: XButtonEvent) -> CompResult<bool> {
        if ev.window != self.pseudo_globals.win {
            return Ok(false);
        }
        self.button(ev.button, ev.x, ev.y, ev.state)
    }

    fn button(&mut self, button: u32, x: c_int, y: c_int, state: u32) -> CompResult<bool> {
        // bool - should exit?
        // Each button does the same as a key, so they behave alike
        use x11::keysym::*;
        let nobuf = [0; 32];
        match button {
            Button1 => match self.click_at(x, y) {
                Some(Click::Item(index)) => {
                    // like Enter; Ctrl-click like Ctrl-Enter keeps the menu open
                    self.items.as_mut().unwrap().curr = index;
                    self.keyprocess(XK_Return, nobuf, 0, state & ControlMask)
                }
                Some(Click::PrevPage) => self.keyprocess(XK_Prior, nobuf, 0, 0),
                Some(Click::NextPage) => self.keyprocess(XK_Next, nobuf, 0, 0),
                None => Ok(false),
            },
            Button2 => self.paste().map(|_| false),
            Button3 => self.keyprocess(XK_Escape, nobuf, 0, 0),
            Button4 => self.keyprocess(XK_Up, nobuf, 0, 0), // scroll up
            Button5 => self.keyprocess(XK_Down, nobuf, 0, 0), // scroll down
            _ => Ok(false),
        }
    }

    fn motion(&mut self, ev: XMotionEvent) -> CompResult<()> {
        // select the item under the pointer
        if ev.window != self.pseudo_globals.win {
            return Ok(());
        }
        self.motion_at(ev.x, ev.y)
    }

    fn motion_at(&mut self, x: c_int, y: c_int) -> CompResult<()> {
        match self.click_at(x, y) {
            Some(Click::Item(index)) if index != self.items.as_ref().unwrap().curr => {
                self.items.as_mut().unwrap().curr = index;
                self.draw()
            }
            _ => Ok(()),
        }
    }

    fn keyprocess(&mut self, ksym: u32, buf: [u8; 32], len: i32, state: u32) -> CompResult<bool> {
        // bool - should exit
        use x11::keysym::*;
        unsafe {
            match ksym {
                XK_Escape => return Die::stderr("".to_owned()), // exit with error code 1
                XK_Return | XK_KP_Enter => {
                    return if (state & ShiftMask) == 0
                        && self.items.as_mut().unwrap().cached_partitions.len() > 0
                    {
                        let (partition_i, partition) = Partition::decompose(
                            &self.items.as_ref().unwrap().cached_partitions,
                            self,
                        ); // find the current selection
                           // and print
                        self.dispose(
                            self.items.as_ref().unwrap().cached_partitions[partition][partition_i]
                                .text
                                .clone(),
                            (state & ControlMask) == 0,
                        )
                    } else {
                        // if Shift-Enter (or no valid options), print contents exactly as in input and return, ignoring selection
                        self.dispose(self.input.clone(), (state & ControlMask) == 0)
                    };
                }
                XK_Tab => {
                    if self.items.as_mut().unwrap().cached_partitions.len() > 0 {
                        // find the current selection
                        let (partition_i, partition) = Partition::decompose(
                            &self.items.as_ref().unwrap().cached_partitions,
                            self,
                        ); // and autocomplete
                        self.input = self.items.as_mut().unwrap().cached_partitions[partition]
                            [partition_i]
                            .text
                            .clone();
                        self.pseudo_globals.cursor = self.input.graphemes(true).count();
                        self.items.as_mut().unwrap().curr = 0;
                    } else {
                        return Ok(false);
                    }
                }
                XK_Home => {
                    if self.items.as_mut().unwrap().cached_partitions.len() > 0 {
                        self.items.as_mut().unwrap().curr = 0;
                    } else {
                        return Ok(false);
                    }
                }
                XK_End => {
                    if self.items.as_mut().unwrap().cached_partitions.len() > 0 {
                        self.items.as_mut().unwrap().curr = self
                            .items
                            .as_mut()
                            .unwrap()
                            .cached_partitions
                            .iter()
                            .fold(0, |acc, cur| acc + cur.len())
                            - 1;
                    } else {
                        return Ok(false);
                    }
                }
                XK_Next => {
                    // PgDn
                    let (partition_i, partition) =
                        Partition::decompose(&self.items.as_ref().unwrap().cached_partitions, self);
                    if partition + 1 < self.items.as_mut().unwrap().cached_partitions.len() {
                        self.items.as_mut().unwrap().curr +=
                            self.items.as_mut().unwrap().cached_partitions[partition].len()
                                - partition_i;
                    } else {
                        return Ok(false);
                    }
                }
                XK_Prior => {
                    // PgUp
                    let (partition_i, partition) =
                        Partition::decompose(&self.items.as_ref().unwrap().cached_partitions, self);
                    if partition > 0 {
                        self.items.as_mut().unwrap().curr -=
                            self.items.as_mut().unwrap().cached_partitions[partition - 1].len()
                                + partition_i;
                    } else {
                        return Ok(false);
                    }
                }
                XK_Left => {
                    if self.config.lines == 0
                        && self.pseudo_globals.cursor == self.input.graphemes(true).count()
                        && self.items.as_mut().unwrap().curr > 0
                    {
                        self.items.as_mut().unwrap().curr -= 1; // move selection
                    } else {
                        // move cursor
                        if self.pseudo_globals.cursor > 0 {
                            self.pseudo_globals.cursor -= 1;
                        } else {
                            return Ok(false);
                        }
                    }
                }
                XK_Right => {
                    if self.config.lines == 0
                        && self.pseudo_globals.cursor == self.input.graphemes(true).count()
                    {
                        // move selection
                        if self.items.as_mut().unwrap().curr + 1
                            < self
                                .items
                                .as_mut()
                                .unwrap()
                                .cached_partitions
                                .iter()
                                .fold(0, |acc, cur| acc + cur.len())
                        {
                            self.items.as_mut().unwrap().curr += 1;
                        } else {
                            return Ok(false);
                        }
                    } else {
                        // move cursor
                        if self.pseudo_globals.cursor < self.input.len() {
                            self.pseudo_globals.cursor += 1;
                        } else {
                            return Ok(false);
                        }
                    }
                }
                XK_Up => {
                    if self.items.as_mut().unwrap().curr > 0 {
                        self.items.as_mut().unwrap().curr -= 1;
                    } else {
                        return Ok(false);
                    }
                }
                XK_Down => {
                    if self.items.as_mut().unwrap().curr + 1
                        < self
                            .items
                            .as_mut()
                            .unwrap()
                            .cached_partitions
                            .iter()
                            .fold(0, |acc, cur| acc + cur.len())
                    {
                        self.items.as_mut().unwrap().curr += 1;
                    } else {
                        return Ok(false);
                    }
                }
                XK_BackSpace => {
                    if self.pseudo_globals.cursor > 0 {
                        let tmp: String = self.input.drain(..).collect();
                        let mut iter = tmp.graphemes(true);
                        self.input = (&mut iter)
                            .take(self.pseudo_globals.cursor - 1)
                            .collect::<String>();
                        iter.next(); // get rid of one char
                        self.input.push_str(&iter.collect::<String>());
                        self.pseudo_globals.cursor -= 1;
                    } else {
                        return Ok(false);
                    }
                }
                XK_Delete => {
                    if self.pseudo_globals.cursor < self.input.len() {
                        let tmp: String = self.input.drain(..).collect();
                        let mut iter = tmp.graphemes(true);
                        self.input = (&mut iter)
                            .take(self.pseudo_globals.cursor)
                            .collect::<String>();
                        iter.next(); // get rid of one char
                        self.input.push_str(&iter.collect::<String>());
                    } else {
                        return Ok(false);
                    }
                }
                _ => {
                    // all others, assumed to be normal chars
                    if iscntrl(*(buf.as_ptr() as *mut i32)) == 0 {
                        let tmp: String = self.input.drain(..).collect();
                        let mut iter = tmp.graphemes(true);
                        self.input = (&mut iter).take(self.pseudo_globals.cursor).collect();
                        self.pseudo_globals.cursor += buf[..len as usize]
                            .iter()
                            .fold(0, |acc, c| acc + if *c > 0 { 1 } else { 0 });
                        self.input
                            .push_str(&String::from_utf8_lossy(&buf[..len as usize]));
                        self.input.push_str(&iter.collect::<String>());
                        self.items.as_mut().unwrap().curr = 0;
                    } else {
                        return Ok(false);
                    }
                }
            }
            self.draw()?;
        }
        Ok(false)
    }

    fn paste(&mut self) -> CompResult<()> {
        // paste selection and redraw
        let mut ctx = match Clipboard::new() {
            Ok(ctx) => ctx,
            Err(_) => return Die::stderr("Could not grab clipboard".to_owned()),
        };
        match ctx.get_text() {
            Ok(mut clip) => {
                clip = match Regex::new(r"[\t]") {
                    Ok(re) => re,
                    Err(_) => return Die::stderr("Cannot build regex".to_owned()),
                }
                .replace_all(
                    &match Regex::new(r"[\r\n]") {
                        Ok(re) => re,
                        Err(_) => return Die::stderr("Cannot build regex".to_owned()),
                    }
                    .replace_all(&clip, "")
                    .to_string(), // remove newlines
                    "    ",
                )
                .to_string(); // replace tab with 4 spaces
                let mut iter = self.input.drain(..).collect::<Vec<char>>().into_iter();
                self.input = (&mut iter).take(self.pseudo_globals.cursor).collect();
                self.input.push_str(&clip);
                self.input.push_str(&iter.collect::<String>());
                self.pseudo_globals.cursor += clip.len();
                self.draw()
            }
            Err(err) => return Die::stderr(err.to_string()),
        }
    }
}
