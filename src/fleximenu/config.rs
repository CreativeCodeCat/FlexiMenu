use libc::{c_int, c_uint};
use x11::xlib::Window;

pub enum Schemes {
    SchemeNorm,
    SchemeSel,
    SchemeOut,
    SchemeHighlight,
    SchemeHover,
    SchemeGreen,
    SchemeRed,
    SchemeYellow,
    SchemeBlue,
    SchemePurple,
    SchemeLast,
}
pub enum Clrs {
    ColFg,
    ColBg,
}
pub use Clrs::*;
pub use Schemes::*;

/// render_minheight value for `-h -1`: 2.5 times the font height
pub const MINHEIGHT_AUTO: u32 = u32::MAX;

#[derive(Debug, PartialEq)]
pub enum DefaultWidth {
    Min,
    Items,
    Max,
    Custom(u8),
}

#[derive(Debug)]
pub struct Config {
    pub lines: c_uint,
    pub topbar: bool,
    pub prompt: String,
    pub promptw: c_int,
    pub fontstrings: Vec<String>,
    pub fast: bool,
    pub embed: Window,
    pub case_sensitive: bool,
    pub mon: c_int,
    pub xoffset: c_int,
    pub yoffset: c_int,
    pub menu_width: c_uint,
    pub colors: [[[u8; 8]; 2]; SchemeLast as usize],
    pub render_minheight: u32,
    pub render_overrun: bool,
    pub render_flex: bool,
    pub render_rightalign: bool,
    pub render_default_width: DefaultWidth,
    pub nostdin: bool,
}

pub struct ConfigDefault {}

impl Default for Config {
    fn default() -> Self {
        Self {
            lines: ConfigDefault::lines(),
            topbar: ConfigDefault::topbar(),
            prompt: ConfigDefault::prompt(),
            promptw: 0,
            fontstrings: ConfigDefault::fontstrings(),
            fast: ConfigDefault::fast(),
            embed: ConfigDefault::embed(),
            case_sensitive: ConfigDefault::case_sensitive(),
            mon: ConfigDefault::mon(),
            xoffset: ConfigDefault::xoffset(),
            yoffset: ConfigDefault::yoffset(),
            menu_width: ConfigDefault::menu_width(),
            colors: ConfigDefault::colors(),
            render_minheight: ConfigDefault::render_minheight(),
            render_overrun: ConfigDefault::render_overrun(),
            render_flex: ConfigDefault::render_flex(),
            render_rightalign: ConfigDefault::render_rightalign(),
            render_default_width: ConfigDefault::render_default_width(),
            nostdin: ConfigDefault::nostdin(),
        }
    }
}
