//! Colour themes, glyphs, and shared style helpers.
//!
//! A [`Theme`] is a flat set of truecolor values; [`ThemeKind`] enumerates the
//! built-in palettes and is what gets persisted in the config. The *active*
//! theme lives in a thread-local cell so the renderer can read it without
//! threading a `&Theme` through every function — call the accessor functions
//! ([`accent`], [`fg`], …) which read the current theme.

use std::cell::Cell;

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

use crate::model::Priority;

/// A complete colour palette.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub sel: Color,
    pub sel_dim: Color,
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub purple: Color,
    pub green: Color,
    pub amber: Color,
    pub red: Color,
    pub teal: Color,
}

impl Theme {
    pub fn priority_color(&self, p: Priority) -> Color {
        match p {
            Priority::High => self.red,
            Priority::Medium => self.amber,
            Priority::Low => self.teal,
        }
    }

    pub fn selection(&self, focused: bool) -> Style {
        if focused {
            Style::default().bg(self.sel).add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(self.sel_dim)
        }
    }

    pub fn pane_border(&self, focused: bool) -> Style {
        if focused {
            Style::default()
                .fg(self.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.muted)
        }
    }
}

/// The built-in palettes, persisted in the config as a kebab-case string.
/// `Terminal` uses the terminal's own ANSI palette and default background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeKind {
    #[default]
    TokyoNight,
    CatppuccinMocha,
    Dracula,
    Nord,
    GruvboxDark,
    SolarizedDark,
    OneDark,
    RosePine,
    Gotham,
    BlackWhite,
    Terminal,
}

impl ThemeKind {
    pub fn all() -> [ThemeKind; 11] {
        [
            ThemeKind::TokyoNight,
            ThemeKind::CatppuccinMocha,
            ThemeKind::Dracula,
            ThemeKind::Nord,
            ThemeKind::GruvboxDark,
            ThemeKind::SolarizedDark,
            ThemeKind::OneDark,
            ThemeKind::RosePine,
            ThemeKind::Gotham,
            ThemeKind::BlackWhite,
            ThemeKind::Terminal,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            ThemeKind::TokyoNight => "Tokyo Night",
            ThemeKind::CatppuccinMocha => "Catppuccin Mocha",
            ThemeKind::Dracula => "Dracula",
            ThemeKind::Nord => "Nord",
            ThemeKind::GruvboxDark => "Gruvbox Dark",
            ThemeKind::SolarizedDark => "Solarized Dark",
            ThemeKind::OneDark => "One Dark",
            ThemeKind::RosePine => "Rosé Pine",
            ThemeKind::Gotham => "Gotham",
            ThemeKind::BlackWhite => "Black & White",
            ThemeKind::Terminal => "Terminal (your colours)",
        }
    }

    /// Next theme in the cycle (wraps).
    pub fn next(self) -> ThemeKind {
        let all = ThemeKind::all();
        let i = all.iter().position(|&k| k == self).unwrap_or(0);
        all[(i + 1) % all.len()]
    }

    /// Parse a user/env string (e.g. "tokyo-night", "dracula", "none").
    pub fn from_key(s: &str) -> Option<ThemeKind> {
        match s.trim().to_lowercase().replace([' ', '_'], "-").as_str() {
            "tokyo-night" | "tokyonight" | "tokyo" => Some(ThemeKind::TokyoNight),
            "catppuccin-mocha" | "catppuccin" | "mocha" => Some(ThemeKind::CatppuccinMocha),
            "dracula" => Some(ThemeKind::Dracula),
            "nord" => Some(ThemeKind::Nord),
            "gruvbox-dark" | "gruvbox" => Some(ThemeKind::GruvboxDark),
            "solarized-dark" | "solarized" => Some(ThemeKind::SolarizedDark),
            "one-dark" | "onedark" | "one" => Some(ThemeKind::OneDark),
            "rose-pine" | "rosepine" | "rose" => Some(ThemeKind::RosePine),
            "gotham" => Some(ThemeKind::Gotham),
            "black-white" | "black-and-white" | "bw" | "mono" | "monochrome" => {
                Some(ThemeKind::BlackWhite)
            }
            "terminal" | "none" | "system" | "default" | "ansi" => Some(ThemeKind::Terminal),
            _ => None,
        }
    }

    pub fn theme(self) -> Theme {
        match self {
            ThemeKind::TokyoNight => TOKYO_NIGHT,
            ThemeKind::CatppuccinMocha => CATPPUCCIN_MOCHA,
            ThemeKind::Dracula => DRACULA,
            ThemeKind::Nord => NORD,
            ThemeKind::GruvboxDark => GRUVBOX_DARK,
            ThemeKind::SolarizedDark => SOLARIZED_DARK,
            ThemeKind::OneDark => ONE_DARK,
            ThemeKind::RosePine => ROSE_PINE,
            ThemeKind::Gotham => GOTHAM,
            ThemeKind::BlackWhite => BLACK_WHITE,
            ThemeKind::Terminal => TERMINAL,
        }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

pub const TOKYO_NIGHT: Theme = Theme {
    bg: rgb(26, 27, 38),
    surface: rgb(22, 22, 30),
    sel: rgb(42, 47, 69),
    sel_dim: rgb(34, 36, 54),
    fg: rgb(192, 202, 245),
    muted: rgb(86, 95, 137),
    accent: rgb(122, 162, 247),
    purple: rgb(187, 154, 247),
    green: rgb(158, 206, 106),
    amber: rgb(224, 175, 104),
    red: rgb(247, 118, 142),
    teal: rgb(125, 207, 255),
};

pub const CATPPUCCIN_MOCHA: Theme = Theme {
    bg: rgb(30, 30, 46),
    surface: rgb(24, 24, 37),
    sel: rgb(49, 50, 68),
    sel_dim: rgb(41, 44, 60),
    fg: rgb(205, 214, 244),
    muted: rgb(108, 112, 134),
    accent: rgb(203, 166, 247),
    purple: rgb(245, 194, 231),
    green: rgb(166, 227, 161),
    amber: rgb(250, 179, 135),
    red: rgb(243, 139, 168),
    teal: rgb(137, 220, 235),
};

pub const NORD: Theme = Theme {
    bg: rgb(46, 52, 64),
    surface: rgb(39, 43, 53),
    sel: rgb(59, 66, 82),
    sel_dim: rgb(52, 58, 70),
    fg: rgb(216, 222, 233),
    muted: rgb(105, 115, 135),
    accent: rgb(136, 192, 208),
    purple: rgb(180, 142, 173),
    green: rgb(163, 190, 140),
    amber: rgb(235, 203, 139),
    red: rgb(191, 97, 106),
    teal: rgb(143, 188, 187),
};

pub const GRUVBOX_DARK: Theme = Theme {
    bg: rgb(40, 40, 40),
    surface: rgb(29, 32, 33),
    sel: rgb(60, 56, 54),
    sel_dim: rgb(50, 48, 47),
    fg: rgb(235, 219, 178),
    muted: rgb(146, 131, 116),
    accent: rgb(250, 189, 47),
    purple: rgb(211, 134, 155),
    green: rgb(184, 187, 38),
    amber: rgb(254, 128, 25),
    red: rgb(251, 73, 52),
    teal: rgb(142, 192, 124),
};

pub const DRACULA: Theme = Theme {
    bg: rgb(40, 42, 54),
    surface: rgb(33, 34, 44),
    sel: rgb(68, 71, 90),
    sel_dim: rgb(52, 55, 70),
    fg: rgb(248, 248, 242),
    muted: rgb(98, 114, 164),
    accent: rgb(189, 147, 249),
    purple: rgb(255, 121, 198),
    green: rgb(80, 250, 123),
    amber: rgb(255, 184, 108),
    red: rgb(255, 85, 85),
    teal: rgb(139, 233, 253),
};

pub const SOLARIZED_DARK: Theme = Theme {
    bg: rgb(0, 43, 54),
    surface: rgb(7, 54, 66),
    sel: rgb(7, 54, 66),
    sel_dim: rgb(3, 51, 61),
    fg: rgb(131, 148, 150),
    muted: rgb(88, 110, 117),
    accent: rgb(38, 139, 210),
    purple: rgb(108, 113, 196),
    green: rgb(133, 153, 0),
    amber: rgb(181, 137, 0),
    red: rgb(220, 50, 47),
    teal: rgb(42, 161, 152),
};

pub const ONE_DARK: Theme = Theme {
    bg: rgb(40, 44, 52),
    surface: rgb(33, 37, 43),
    sel: rgb(62, 68, 81),
    sel_dim: rgb(44, 49, 58),
    fg: rgb(171, 178, 191),
    muted: rgb(92, 99, 112),
    accent: rgb(97, 175, 239),
    purple: rgb(198, 120, 221),
    green: rgb(152, 195, 121),
    amber: rgb(229, 192, 123),
    red: rgb(224, 108, 117),
    teal: rgb(86, 182, 194),
};

pub const ROSE_PINE: Theme = Theme {
    bg: rgb(25, 23, 36),
    surface: rgb(31, 29, 46),
    sel: rgb(38, 35, 58),
    sel_dim: rgb(33, 32, 46),
    fg: rgb(224, 222, 244),
    muted: rgb(110, 106, 134),
    accent: rgb(196, 167, 231),
    purple: rgb(235, 188, 186),
    green: rgb(156, 207, 216),
    amber: rgb(246, 193, 119),
    red: rgb(235, 111, 146),
    teal: rgb(49, 116, 143),
};

pub const GOTHAM: Theme = Theme {
    bg: rgb(12, 16, 20),
    surface: rgb(10, 14, 18),
    sel: rgb(10, 55, 73),
    sel_dim: rgb(17, 21, 28),
    fg: rgb(153, 209, 206),
    muted: rgb(36, 83, 97),
    accent: rgb(89, 156, 171),
    purple: rgb(136, 140, 166),
    green: rgb(42, 168, 137),
    amber: rgb(237, 180, 67),
    red: rgb(194, 49, 39),
    teal: rgb(51, 133, 158),
};

/// High-contrast light monochrome.
pub const BLACK_WHITE: Theme = Theme {
    bg: rgb(255, 255, 255),
    surface: rgb(238, 238, 238),
    sel: rgb(205, 205, 205),
    sel_dim: rgb(228, 228, 228),
    fg: rgb(17, 17, 17),
    muted: rgb(128, 128, 128),
    accent: rgb(0, 0, 0),
    purple: rgb(68, 68, 68),
    green: rgb(51, 51, 51),
    amber: rgb(90, 90, 90),
    red: rgb(0, 0, 0),
    teal: rgb(110, 110, 110),
};

/// "Follow the terminal": no forced backgrounds (`Reset` = terminal default),
/// and the 16 ANSI colours so accents track the user's own terminal palette.
pub const TERMINAL: Theme = Theme {
    bg: Color::Reset,
    surface: Color::Reset,
    sel: Color::DarkGray,
    sel_dim: Color::Black,
    fg: Color::Reset,
    muted: Color::DarkGray,
    accent: Color::Cyan,
    purple: Color::Magenta,
    green: Color::Green,
    amber: Color::Yellow,
    red: Color::Red,
    teal: Color::Blue,
};

// --- active theme (thread-local) --------------------------------------------

thread_local! {
    static CURRENT: Cell<Theme> = const { Cell::new(TOKYO_NIGHT) };
}

/// Replace the active theme used by the renderer.
pub fn set(theme: Theme) {
    CURRENT.with(|c| c.set(theme));
}

/// The active theme.
pub fn current() -> Theme {
    CURRENT.with(|c| c.get())
}

// --- accessors used by the renderer -----------------------------------------

pub fn bg() -> Color {
    current().bg
}
pub fn surface() -> Color {
    current().surface
}
pub fn sel_dim() -> Color {
    current().sel_dim
}
pub fn fg() -> Color {
    current().fg
}
pub fn muted() -> Color {
    current().muted
}
pub fn accent() -> Color {
    current().accent
}
pub fn purple() -> Color {
    current().purple
}
pub fn green() -> Color {
    current().green
}
pub fn amber() -> Color {
    current().amber
}
pub fn red() -> Color {
    current().red
}
pub fn teal() -> Color {
    current().teal
}

pub fn priority_color(p: Priority) -> Color {
    current().priority_color(p)
}
pub fn selection(focused: bool) -> Style {
    current().selection(focused)
}
pub fn pane_border(focused: bool) -> Style {
    current().pane_border(focused)
}

// --- glyphs (theme-independent) ---------------------------------------------

pub const CHECK_DONE: &str = "\u{2713}"; // ✓
pub const CHECK_OPEN: &str = "\u{25cb}"; // ○
pub const DOT: &str = "\u{25cf}"; // ●
pub const FLAG: &str = "\u{2691}"; // ⚑
pub const DIAMOND: &str = "\u{25c6}"; // ◆
// Progress bar uses box-drawing lines (same family as the pane borders) rather
// than shade blocks (▓░), which many terminal fonts don't render.
pub const BAR_FULL: &str = "\u{2501}"; // ━ heavy horizontal
pub const BAR_EMPTY: &str = "\u{2500}"; // ─ light horizontal
