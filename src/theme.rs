use iocraft::prelude::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub border: Color,
    pub text: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub project: Color,
    pub framework: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub selected_bg: Color,
}

fn rgb(hex: u32) -> Color {
    Color::Rgb {
        r: ((hex >> 16) & 0xFF) as u8,
        g: ((hex >> 8) & 0xFF) as u8,
        b: (hex & 0xFF) as u8,
    }
}

impl Theme {
    pub fn tokyo_night() -> Self {
        Self {
            name: "Tokyo Night",
            bg: rgb(0x1a1b26),
            border: rgb(0x565f89),
            text: Color::White,
            text_muted: rgb(0x565f89),
            accent: rgb(0x7dcfff),
            project: rgb(0x7adcc3),
            framework: rgb(0xbb9af7),
            success: rgb(0x9ece6a),
            warning: rgb(0xe0af68),
            error: rgb(0xf7768e),
            selected_bg: rgb(0x24283b),
        }
    }

    pub fn catppuccin() -> Self {
        Self {
            name: "Catppuccin",
            bg: rgb(0x1e1e2e),
            border: rgb(0x6c7086),
            text: Color::White,
            text_muted: rgb(0x6c7086),
            accent: rgb(0x89b4fa),
            project: rgb(0xa6e3a1),
            framework: rgb(0xcba6f7),
            success: rgb(0xa6e3a1),
            warning: rgb(0xf9e2af),
            error: rgb(0xf38ba8),
            selected_bg: rgb(0x313244),
        }
    }

    pub fn rosepine() -> Self {
        Self {
            name: "Rosepine",
            bg: rgb(0x191724),
            border: rgb(0x6e6a86),
            text: Color::White,
            text_muted: rgb(0x6e6a86),
            accent: rgb(0x9ccfd8),
            project: rgb(0xf6c177),
            framework: rgb(0xc4a7e7),
            success: rgb(0x9ccfd8),
            warning: rgb(0xf6c177),
            error: rgb(0xeb6f92),
            selected_bg: rgb(0x26233a),
        }
    }

    pub fn light() -> Self {
        Self {
            name: "Light",
            bg: rgb(0xeff1f5),
            border: rgb(0xacb0be),
            text: Color::Reset,
            text_muted: rgb(0x7c7f93),
            accent: rgb(0x1e66f5),
            project: rgb(0x40a02b),
            framework: rgb(0x8839ef),
            success: rgb(0x40a02b),
            warning: rgb(0xdf8e1d),
            error: rgb(0xd20f39),
            selected_bg: rgb(0xe6e9ef),
        }
    }

    pub fn dark() -> Self {
        Self::tokyo_night()
    }
}

const ALL_THEMES: [fn() -> Theme; 4] = [
    Theme::tokyo_night,
    Theme::catppuccin,
    Theme::rosepine,
    Theme::light,
];

pub fn next_theme(current_name: &str) -> Theme {
    let idx = ALL_THEMES
        .iter()
        .position(|f| f().name == current_name)
        .unwrap_or(0);
    ALL_THEMES[(idx + 1) % ALL_THEMES.len()]()
}

/// Detect terminal background from COLORFGBG env var.
/// Format: "fg;bg" where bg < 7 = dark, >= 7 = light.
/// Falls back to dark if unset or unparseable.
fn detect_background() -> Theme {
    let bg = std::env::var("COLORFGBG")
        .ok()
        .and_then(|v| v.rsplit(';').next().map(String::from))
        .and_then(|s| s.parse::<u8>().ok());

    match bg {
        Some(n) if n >= 7 => Theme::light(),
        _ => Theme::dark(),
    }
}

pub fn resolve(choice: &str) -> Theme {
    match choice {
        "light" => Theme::light(),
        "dark" => Theme::dark(),
        "catppuccin" => Theme::catppuccin(),
        "rosepine" => Theme::rosepine(),
        "tokyo-night" => Theme::tokyo_night(),
        _ => detect_background(),
    }
}
