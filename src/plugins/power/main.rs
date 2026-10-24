use overrider::*;
use std::process::Command;

use crate::clapflags::CLAP_FLAGS;
use crate::config::ConfigDefault;
use crate::drw::Drw;
use crate::result::*;

struct Action {
    icon: &'static str,  // Nerd Font glyph
    color: char,         // emojihighlight scheme for the icon box
    label: &'static str,
    command: &'static str, // run with sh -c
}

// Shown in this order, so the first (default) choice is the harmless one
const ACTIONS: [Action; 6] = [
    Action { icon: "\u{f023}", color: 'b', label: "Lock", command: "loginctl lock-session" },
    Action {
	icon: "\u{f08b}",
	color: 'p',
	label: "Logout",
	command: "loginctl terminate-session \"${XDG_SESSION_ID:-self}\"",
    },
    Action { icon: "\u{f186}", color: 'g', label: "Suspend", command: "systemctl suspend" },
    Action { icon: "\u{f2dc}", color: 'h', label: "Hibernate", command: "systemctl hibernate" },
    Action { icon: "\u{f021}", color: 'y', label: "Reboot", command: "systemctl reboot" },
    Action { icon: "\u{f011}", color: 'r', label: "Shutdown", command: "systemctl poweroff" },
];

/// Is the emojihighlight plugin compiled in and enabled?
fn highlighting() -> bool {
    env!("PLUGINS").split(' ').any(|p| p == "emojihighlight")
	&& CLAP_FLAGS.occurrences_of("nohighlight") == 0
}

/// Menu item text for an action
fn item_text(action: &Action) -> String {
    if highlighting() {
	format!(":{} {} {}", action.color, action.icon, action.label)
    } else {
	format!("{}  {}", action.icon, action.label)
    }
}

#[override_flag(flag = power)]
impl Drw {
    pub fn format_stdin(&mut self, _lines: Vec<String>) -> CompResult<Vec<String>> {
	Ok(ACTIONS.iter().map(item_text).collect())
    }
    pub fn dispose(&mut self, output: String, _recommendation: bool) -> CompResult<bool> {
	// Selected items match exactly, typed input (Shift-Enter) by label
	let action = ACTIONS
	    .iter()
	    .find(|a| output == item_text(a) || output.trim().eq_ignore_ascii_case(a.label))
	    .ok_or(Die::Stderr(format!("Unknown power action: {}", output)))?;
	Command::new("sh")
	    .arg("-c")
	    .arg(action.command)
	    .spawn()
	    .map_err(|_| Die::Stderr("Failed to spawn child process".to_owned()))?;
	Ok(true)
    }
}

#[override_flag(flag = power)]
impl ConfigDefault {
    pub fn nostdin() -> bool {
	true // the menu items come from format_stdin instead
    }
    pub fn lines() -> u32 {
	ACTIONS.len() as u32 // vertical, so the icon boxes don't squeeze the labels
    }
    pub fn case_sensitive() -> bool {
	false // so typing "shut" finds Shutdown
    }
}
