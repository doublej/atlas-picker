use iocraft::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

const LOGOS: &[&[&str]] = &[
    // SLANT
    &[
        "           _      __            ",
        "    ____  (_)____/ /_____  _____",
        "   / __ \\/ / ___/ //_/ _ \\/ ___/",
        "  / /_/ / / /__/ ,< /  __/ /    ",
        " / .___/_/\\___/_/|_|\\___/_/     ",
        "/_/                             ",
    ],
    // LEAN
    &[
        "                _/            _/                            ",
        "     _/_/_/          _/_/_/  _/  _/      _/_/    _/  _/_/  ",
        "    _/    _/  _/  _/        _/_/      _/_/_/_/  _/_/       ",
        "   _/    _/  _/  _/        _/  _/    _/        _/          ",
        "  _/_/_/    _/    _/_/_/  _/    _/    _/_/_/  _/           ",
        " _/                                                        ",
    ],
    // SHADOW
    &[
        "      _)      |              ",
        " __ \\  |  __| |  /  _ \\  __|",
        " |   | | (      <   __/ |    ",
        " .__/ _|\\___|_|\\_\\___|_|    ",
        "_|                           ",
    ],
    // SMSLANT
    &[
        "         _     __          ",
        "   ___  (_)___/ /_____ ____",
        "  / _ \\/ / __/  '_/ -_) __/",
        " / .__/_/\\__/_/\\_\\\\__/_/   ",
        "/_/                        ",
    ],
    // LETTERS
    &[
        "        iii        kk                   ",
        "pp pp         cccc kk  kk   eee  rr rr  ",
        "ppp  pp iii cc     kkkkk  ee   e rrr  r ",
        "pppppp  iii cc     kk kk  eeeee  rr     ",
        "pp      iii  ccccc kk  kk  eeeee rr     ",
    ],
];

const SUBTITLE: &str = "find. pick. go.";

fn pick_logo() -> &'static [&'static str] {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let idx = (nanos % LOGOS.len() as u128) as usize;
    LOGOS[idx]
}

#[derive(Default, Props)]
pub struct SplashProps {
    pub visible: bool,
    pub color: Option<Color>,
    pub accent: Option<Color>,
}

#[component]
pub fn Splash(props: &SplashProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element!(View);
    }
    let color = props.color.unwrap_or(Color::DarkGrey);
    let accent = props.accent.unwrap_or(color);
    let logo = pick_logo();

    element! {
        View(
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::End,
        ) {
            // Wrap logo in a left-aligned block so it centers as one unit.
            View(flex_direction: FlexDirection::Column, align_items: AlignItems::Start) {
                #(logo.iter().map(|line| {
                    element! {
                        Text(content: line.to_string(), color: accent)
                    }
                }))
            }
            View(height: 1u32)
            Text(content: SUBTITLE, color: color)
        }
    }
}
