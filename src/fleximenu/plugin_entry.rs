#[allow(unused_imports)]
use crate::clapflags::CLAP_FLAGS;
use crate::drw::Drw;
#[allow(unused_imports)]
use crate::item::{Item, MatchCode};
#[allow(unused_imports)]
use crate::result::*;

use libc::c_int;
use overrider::*;
#[allow(unused_imports)]
use regex::{Regex, RegexBuilder};

use crate::config::ConfigDefault;
use crate::config::DefaultWidth;
use crate::config::Schemes::*;

#[default]
impl Drw {
    /**
     * When taking input from stdin, apply post-processing
     * With nostdin, this gets no lines and can supply the menu items itself
     */
    pub fn format_stdin(&mut self, lines: Vec<String>) -> CompResult<Vec<String>> {
        Ok(lines)
    }

    /**
     * Every time the input is drawn, how should it be presented?
     * Does it need additional processing?
     */
    pub fn format_input(&mut self) -> CompResult<String> {
        Ok(self.input.clone())
    }

    /**
     * What to do when printing to stdout / program termination?
     *
     * Args:
     * - output: what's being processed
     * - recommendation: is exiting recommended? C-Enter will not normally exit
     *
     * Returns - true if program should exit
     */
    pub fn dispose(&mut self, output: String, recommendation: bool) -> CompResult<bool> {
        println!("{}", output);
        Ok(recommendation)
    }

    /**
     * The following is called immediatly after gen_matches, taking its unwrapped output
     *
     * This is particularly useful for doing something based on a match method defined
     * elsewhere. For example, if any matched items contain a key, highlight them,
     * but still allow a custom matching algorithm (such as from the fuzzy plugin)
     */
    pub fn postprocess_matches(&mut self, items: Vec<Item>) -> CompResult<Vec<Item>> {
        Ok(items)
    }

    /**
     * Every time the input changes, what items should be shown
     * And, how should they be shown?
     *
     * Returns - Vector of items to be drawn
     */
    pub fn gen_matches(&mut self) -> CompResult<Vec<Item>> {
        let re = RegexBuilder::new(&regex::escape(&self.input))
            .case_insensitive(!self.config.case_sensitive)
            .build()
            .map_err(|_| Die::Stderr("Could not build regex".to_owned()))?;
        let mut exact: Vec<Item> = Vec::new();
        let mut prefix: Vec<Item> = Vec::new();
        let mut substring: Vec<Item> = Vec::new();
        for item in self.get_items() {
            match item.matches(&re) {
                MatchCode::Exact => exact.push(item.clone()),
                MatchCode::Prefix => prefix.push(item.clone()),
                MatchCode::Substring => substring.push(item.clone()),
                MatchCode::None => {}
            }
        }
        exact.reserve(prefix.len() + substring.len());
        for item in prefix {
            // extend is broken for pointers
            exact.push(item);
        }
        for item in substring {
            exact.push(item);
        }
        Ok(exact)
    }

    /**
     * How should a single menu item be drawn?
     *
     * Args:
     * - item: the item being drawn
     * - x, y, w: where to draw it, and how much width it may take up
     * - selected: is this the currently selected item?
     *
     * Returns - x coordinate just past the drawn item
     */
    pub fn draw_item(
        &mut self,
        item: &Item,
        x: c_int,
        y: c_int,
        w: c_int,
        selected: bool,
    ) -> CompResult<c_int> {
        if selected {
            self.setscheme(SchemeSel);
        } else if item.out {
            self.setscheme(SchemeOut);
        } else {
            self.setscheme(SchemeNorm);
        }
        item.draw(x, y, w, self)
    }
}

/// The following are the default config values, loaded just after program init
#[default]
impl ConfigDefault {
    pub fn lines() -> u32 {
        0
    }
    pub fn topbar() -> bool {
        true
    }
    pub fn center() -> bool {
        false
    }
    pub fn mouse() -> bool {
        false
    }
    pub fn min_width() -> u32 {
        300 // smallest width of a centered or mouse menu
    }
    pub fn prompt() -> String {
        String::new()
    }
    pub fn fontstrings() -> Vec<String> {
        vec!["mono:size=10".to_owned()]
    }
    pub fn fast() -> bool {
        false
    }
    pub fn embed() -> u64 {
        0
    }
    pub fn case_sensitive() -> bool {
        true
    }
    pub fn mon() -> i32 {
        -1
    }
    pub fn xoffset() -> i32 {
        0
    }
    pub fn yoffset() -> i32 {
        0
    }
    pub fn menu_width() -> u32 {
        0 // 0 means full screen width
    }
    pub fn colors() -> [[[u8; 8]; 2]; SchemeLast as usize] {
        // KDE's Breeze Dark colors (BreezeDark.colors)
        /*                         [  fg             bg         ]*/
        let mut arr = [[[0; 8]; 2]; SchemeLast as usize];
        arr[SchemeNorm as usize] = [*b"#fcfcfc\0", *b"#202326\0"]; // window
        arr[SchemeSel as usize] = [*b"#fcfcfc\0", *b"#3daee9\0"]; // selection
        arr[SchemeOut as usize] = [*b"#fcfcfc\0", *b"#1e5774\0"]; // selection alternate
        arr[SchemeHighlight as usize] = [*b"#fdbc4b\0", *b"#292c30\0"]; // button
        arr[SchemeHover as usize] = [*b"#fcfcfc\0", *b"#1e5774\0"]; // selection alternate
        arr[SchemeGreen as usize] = [*b"#fcfcfc\0", *b"#27ae60\0"]; // positive
        arr[SchemeRed as usize] = [*b"#fcfcfc\0", *b"#da4453\0"]; // negative
        arr[SchemeYellow as usize] = [*b"#202326\0", *b"#fdbc4b\0"]; // yellow
        arr[SchemeBlue as usize] = [*b"#fcfcfc\0", *b"#1d99f3\0"]; // link
        arr[SchemePurple as usize] = [*b"#fcfcfc\0", *b"#9b59b6\0"]; // visited
        arr
    }
    pub fn nostdin() -> bool {
        false
    }
    pub fn render_minheight() -> u32 {
        4
    }
    pub fn render_overrun() -> bool {
        false
    }
    pub fn render_flex() -> bool {
        false
    }
    pub fn render_rightalign() -> bool {
        false
    }
    pub fn render_default_width() -> DefaultWidth {
        DefaultWidth::Items
    }
}
