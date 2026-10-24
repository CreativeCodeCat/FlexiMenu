use libc::{c_char, c_int, c_long, c_void};
use std::mem::MaybeUninit;
use std::ptr;
use x11::xinerama::{XineramaQueryScreens, XineramaScreenInfo};
use x11::xlib::{
    CWBackPixel, CWEventMask, CWOverrideRedirect, ExposureMask, FocusChangeMask, KeyPressMask,
    PointerRoot, SubstructureNotifyMask, VisibilityChangeMask, Window, XClassHint, XCreateIC,
    XCreateWindow, XFree, XGetInputFocus, XGetWindowAttributes, XIMPreeditNothing,
    XIMStatusNothing, XMapRaised, XOpenIM, XQueryPointer, XQueryTree, XSelectInput, XSetClassHint,
    XSetWindowAttributes,
};

use crate::additional_bindings::xlib::{XNClientWindow, XNFocusWindow, XNInputStyle};
use crate::config::{Clrs::*, Schemes::*, MINHEIGHT_AUTO};
use crate::drw::{Drw, TextOption::Prompt};
use crate::result::*;
use crate::util::grabfocus;

#[inline]
fn intersect(x: c_int, y: c_int, w: c_int, h: c_int, r: *mut XineramaScreenInfo) -> c_int {
    unsafe {
        0.max((x + w).min(((*r).x_org + (*r).width) as c_int) - x.max((*r).x_org as c_int))
            * 0.max((y + h).min(((*r).y_org + (*r).height) as c_int) - y.max((*r).y_org as c_int))
    }
}

impl Drw {
    /// Sets the menu width and returns its position within the given screen area
    fn place(
        &mut self,
        area_x: c_int,
        area_y: c_int,
        area_w: c_int,
        area_h: c_int,
    ) -> CompResult<(c_int, c_int)> {
        if self.config.center {
            // as wide as the widest item and prompt, but no smaller than min_width
            self.w = if self.config.menu_width > 0 {
                self.config.menu_width as c_int
            } else {
                let promptw = if self.config.prompt.len() > 0 {
                    self.textw(Prompt)?
                } else {
                    0
                };
                let itemw = self.get_items().iter().map(|i| i.width).max().unwrap_or(0);
                (itemw + promptw).max(self.config.min_width as c_int)
            }
            .min(area_w);
            Ok((
                area_x + (area_w - self.w) / 2 + self.config.xoffset,
                area_y + (area_h - self.h) / 2 + self.config.yoffset,
            ))
        } else {
            self.w = if self.config.menu_width > 0 {
                self.config.menu_width as c_int
            } else {
                area_w
            };
            Ok((
                area_x + self.config.xoffset,
                area_y
                    + if self.config.topbar {
                        self.config.yoffset
                    } else {
                        area_h - self.h - self.config.yoffset
                    },
            ))
        }
    }

    pub fn setup(&mut self, parentwin: u64, root: u64) -> CompResult<()> {
        unsafe {
            let mut x: c_int;
            let mut y: c_int;

            let mut ch: XClassHint = XClassHint {
                res_name: (*b"fleximenu\0").as_ptr() as *mut c_char,
                res_class: (*b"fleximenu\0").as_ptr() as *mut c_char,
            };

            // appearances are set up in constructor

            let minheight = if self.config.render_minheight == MINHEIGHT_AUTO {
                (self.fonts[0].height as f32 * 2.5) as u32
            } else {
                self.config.render_minheight
            };
            self.pseudo_globals.bh =
                (self.fonts.iter().map(|f| f.height).max().unwrap() + 4).max(minheight);
            self.h = ((self.config.lines + 1) * self.pseudo_globals.bh) as c_int;

            let mut dws: *mut Window = ptr::null_mut();
            let mut w = MaybeUninit::<Window>::uninit();
            let mut dw = MaybeUninit::<Window>::uninit();
            let n: c_int;
            let info = if cfg!(feature = "Xinerama") && parentwin == root {
                let mut __n = MaybeUninit::uninit();
                let ret = XineramaQueryScreens(self.dpy, __n.as_mut_ptr());
                n = __n.assume_init();
                ret
            } else {
                // Setting n=0 isn't required here, but rustc isn't smart enough
                // to realize that the only use of n can't occur if this branch is taken
                n = 0;
                ptr::null_mut()
            };
            if cfg!(feature = "Xinerama") && info != ptr::null_mut() {
                let mut i = 0;
                let mut area = 0;
                let mut di = MaybeUninit::<c_int>::uninit();
                let mut a;
                let mut pw;

                XGetInputFocus(self.dpy, w.as_mut_ptr(), di.as_mut_ptr());
                if self.config.mon >= 0 && self.config.mon < n {
                    i = self.config.mon;
                } else if w.assume_init() != root
                    && w.assume_init() != PointerRoot as u64
                    && w.assume_init() != 0
                {
                    /* find top-level window containing current input focus */
                    while {
                        pw = w.assume_init();
                        let mut _du = MaybeUninit::uninit();
                        if XQueryTree(
                            self.dpy,
                            pw,
                            dw.as_mut_ptr(),
                            w.as_mut_ptr(),
                            &mut dws,
                            _du.as_mut_ptr(),
                        ) != 0
                            && dws != ptr::null_mut()
                        {
                            XFree(dws as *mut c_void);
                        }
                        w.assume_init() != root && w.assume_init() != pw
                    } {} // do-while
                         /* find xinerama screen with which the window intersects most */
                    if XGetWindowAttributes(self.dpy, pw, &mut self.wa) != 0 {
                        for j in 0..n {
                            a = intersect(
                                self.wa.x,
                                self.wa.y,
                                self.wa.width,
                                self.wa.height,
                                info.offset(j as isize),
                            );
                            if a > area {
                                area = a;
                                i = j;
                            }
                        }
                    }
                }
                /* no focused window is on screen, so use pointer location instead */
                let mut _du = MaybeUninit::uninit();
                let mut __x = MaybeUninit::uninit();
                let mut __y = MaybeUninit::uninit();
                if self.config.mon < 0
                    && area == 0
                    && XQueryPointer(
                        self.dpy,
                        root,
                        dw.as_mut_ptr(),
                        dw.as_mut_ptr(),
                        __x.as_mut_ptr(),
                        __y.as_mut_ptr(),
                        di.as_mut_ptr(),
                        di.as_mut_ptr(),
                        _du.as_mut_ptr(),
                    ) != 0
                {
                    x = __x.assume_init();
                    y = __y.assume_init();
                    for j in 0..n {
                        i = j; // this is here to bypass rust's shadowing rules in an efficient way
                        if intersect(x, y, 1, 1, info.offset(i as isize)) != 0 {
                            break;
                        }
                    }
                }
                let screen = *info.offset(i as isize);
                XFree(info as *mut c_void);
                let (px, py) = self.place(
                    screen.x_org as c_int,
                    screen.y_org as c_int,
                    screen.width as c_int,
                    screen.height as c_int,
                )?;
                x = px;
                y = py;
            } else {
                if XGetWindowAttributes(self.dpy, parentwin, &mut self.wa) == 0 {
                    return Die::stderr(format!(
                        "could not get embedding window attributes: 0x{:?}",
                        parentwin
                    ));
                }
                let (px, py) = self.place(0, 0, self.wa.width, self.wa.height)?;
                x = px;
                y = py;
            }

            let mut swa = XSetWindowAttributes {
                override_redirect: true as i32,
                background_pixel: (*self.pseudo_globals.schemeset[SchemeNorm as usize]
                    [ColBg as usize])
                    .pixel,
                event_mask: ExposureMask | KeyPressMask | VisibilityChangeMask,
                background_pixmap: 0,
                backing_pixel: 0,
                backing_store: 0,
                backing_planes: 0,
                bit_gravity: 0,
                border_pixel: 0,
                border_pixmap: 0,
                colormap: 0,
                cursor: 0,
                do_not_propagate_mask: false as c_long,
                save_under: 0,
                win_gravity: 0,
            };
            self.pseudo_globals.win = XCreateWindow(
                self.dpy,
                parentwin,
                x,
                y,
                self.w as u32,
                self.h as u32,
                0,
                0,
                0,
                ptr::null_mut(),
                CWOverrideRedirect | CWBackPixel | CWEventMask,
                &mut swa,
            );
            XSetClassHint(self.dpy, self.pseudo_globals.win, &mut ch);

            /* input methods */
            let xim = XOpenIM(self.dpy, ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
            if xim == ptr::null_mut() {
                return Die::stderr("XOpenIM failed: could not open input device".to_owned());
            }

            self.pseudo_globals.xic = XCreateIC(
                xim,
                XNInputStyle,
                XIMPreeditNothing | XIMStatusNothing,
                XNClientWindow,
                self.pseudo_globals.win,
                XNFocusWindow,
                self.pseudo_globals.win,
                ptr::null_mut::<c_void>(),
            );
            // void* makes sure the value is large enough for varargs to properly stop
            // parsing. Any smaller and it will skip over, causing a segfault

            XMapRaised(self.dpy, self.pseudo_globals.win);

            if self.config.embed != 0 {
                XSelectInput(
                    self.dpy,
                    parentwin,
                    FocusChangeMask | SubstructureNotifyMask,
                );
                let mut du = MaybeUninit::uninit();
                if XQueryTree(
                    self.dpy,
                    parentwin,
                    dw.as_mut_ptr(),
                    w.as_mut_ptr(),
                    &mut dws,
                    du.as_mut_ptr(),
                ) != 0
                    && dws != ptr::null_mut()
                {
                    for i in 0..du.assume_init() {
                        if *dws.offset(i as isize) == self.pseudo_globals.win {
                            break;
                        }
                        XSelectInput(self.dpy, *dws.offset(i as isize), FocusChangeMask);
                    }
                    XFree(dws as *mut c_void);
                }
                grabfocus(self)?;
            }

            self.draw()
        }
    }
}
