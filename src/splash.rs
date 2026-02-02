use iocraft::prelude::*;

const LOGO: [&str; 6] = [
    "        _      _",
    "  _ __ (_) ___| | _____ _ __",
    " | '_ \\| |/ __| |/ / _ \\ '__|",
    " | |_) | | (__|   <  __/ |",
    " | .__/|_|\\___|_|\\_\\___|_|",
    " |_|",
];

const SUBTITLE: &str = "find. pick. go.";

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

    element! {
        View(
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::End,
        ) {
            // Wrap logo in a left-aligned block so it centers as one unit.
            View(flex_direction: FlexDirection::Column, align_items: AlignItems::Start) {
                #(LOGO.iter().map(|line| {
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
