pub mod header;
pub mod instrument;
pub mod knob;
pub mod master;
pub mod mixer;
pub mod sequencer;

pub const CH_COLORS: [&str; 13] = [
    "#ff5a1f", "#ffb020", "#ffd23f", "#e8e26a", "#7bd389", "#3fc1a5", "#38a3d9", "#4f7cff",
    "#7a6bff", "#b06bff", "#e061c8", "#ff5f8f", "#ff3b3b",
];

/// Labels of the mute keys, by channel (physical positions: number row, then Q W E).
pub const MUTE_KEYS: [&str; 13] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "Q", "W", "E",
];

/// Map a physical key (`KeyboardEvent.code`) to the channel whose mute it toggles.
pub fn mute_key_channel(code: &str) -> Option<usize> {
    const CODES: [&str; 13] = [
        "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9",
        "Digit0", "KeyQ", "KeyW", "KeyE",
    ];
    CODES.iter().position(|&c| c == code)
}
