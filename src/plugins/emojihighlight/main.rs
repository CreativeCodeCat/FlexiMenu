use overrider::*;

#[allow(unused_imports)]
use crate::clapflags::CLAP_FLAGS;

use libc::c_int;
use unicode_segmentation::UnicodeSegmentation;

use crate::config::Schemes::{self, *};
use crate::drw::{Drw, TextOption::Other};
use crate::item::Item;
use crate::result::*;

/// Color scheme picked by the letter in a `>>X` or `:X` prefix
fn prefix_scheme(letter: Option<char>) -> Option<Schemes> {
    match letter? {
        'r' => Some(SchemeRed),
        'g' => Some(SchemeGreen),
        'y' => Some(SchemeYellow),
        'b' => Some(SchemeBlue),
        'p' => Some(SchemePurple),
        'h' => Some(SchemeHighlight),
        's' => Some(SchemeSel),
        _ => None,
    }
}

/// `text` without its first `n` characters
fn skip_chars(text: &str, n: usize) -> &str {
    text.char_indices().nth(n).map_or("", |(i, _)| &text[i..])
}

/// Draws text in the current scheme. Empty text is drawn as a space,
/// since Drw::text skips the background and returns 0 for empty strings
fn draw_text(drw: &mut Drw, x: c_int, y: c_int, w: c_int, lpad: c_int, text: &str) -> CompResult<c_int> {
    let text = if text.is_empty() { " " } else { text };
    drw.text(x, y, w as u32, drw.pseudo_globals.bh, lpad as u32, Other(&text.to_string()), false)
	.map(|o| o.0)
}

#[override_flag(flag = nohighlight, invert = true)]
impl Drw {
    pub fn draw_item(
        &mut self,
        item: &Item,
        x: c_int,
        y: c_int,
        w: c_int,
        selected: bool,
    ) -> CompResult<c_int> {
	let mut chars = item.text.chars();
	let (first, second, third) = (chars.next(), chars.next(), chars.next());
	let lrpad = self.pseudo_globals.lrpad;

	// Pick a scheme and how many prefix characters to hide
	let skip = match first {
	    // `>` comments, `>>X` colored with X. Never drawn as selected
	    Some('>') => match (second, prefix_scheme(third)) {
		(Some('>'), Some(scheme)) => {
		    self.setscheme(scheme);
		    3
		}
		_ => {
		    self.setscheme(SchemeNorm);
		    1
		}
	    },
	    // `:X` items are colored with X while selected
	    Some(':') => {
		if !selected {
		    self.setscheme(SchemeNorm);
		    2
		} else if let Some(scheme) = prefix_scheme(second) {
		    self.setscheme(scheme);
		    2
		} else {
		    self.setscheme(SchemeSel);
		    0
		}
	    }
	    _ => {
		self.setscheme(if selected {
		    SchemeSel
		} else if item.out {
		    SchemeOut
		} else {
		    SchemeNorm
		});
		0
	    }
	};
	let text = skip_chars(&item.text, skip);

	// `:X <emoji>...` puts the emoji in a box of the current scheme,
	// followed by the rest of the item. Only done if there's room for both
	let boxw = lrpad * 3;
	if first == Some(':') && skip == 2 && third == Some(' ') && w > boxw + lrpad {
	    let after_space = skip_chars(text, 1);
	    let emoji = after_space.graphemes(true).next().unwrap_or("");
	    let rest = &after_space[emoji.len()..];

	    let emojiw = self.fontset_getwidth(Other(&emoji.to_string()))?;
	    draw_text(self, x, y, boxw, ((boxw - emojiw) / 2).max(0), emoji)?;
	    self.setscheme(if selected { SchemeHover } else { SchemeNorm });
	    return draw_text(self, x + boxw, y, w - boxw, lrpad / 2, rest);
	}

	draw_text(self, x, y, w, lrpad / 2, text)
    }
}
