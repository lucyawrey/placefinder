use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Box,
    Text,
    Span,
    Button,
    Link,
    TextInput,
    Image,
}

/// One element's fields, as `ui.check` declares them. Fields an element's kind doesn't
/// declare stay at their defaults. A `None` style field takes its default, or for `color` and
/// `font_size` the nearest ancestor's.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Element {
    #[serde(skip)]
    pub kind: Kind,
    pub text: String,
    /// Where a `Link` leads: any target a Portal takes.
    pub target: String,
    /// An `Image`'s file, relative to the Place.
    pub src: String,
    pub value: String,
    pub pressable: bool,
    pub focusable: bool,
    pub direction: Option<Direction>,
    pub gap: Option<Space>,
    pub padding: Option<Space>,
    pub margin: Option<Space>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub align: Option<Align>,
    pub justify: Option<Justify>,
    pub background: Option<Color>,
    pub color: Option<Color>,
    pub font_size: Option<TextSize>,
    pub border: Option<Space>,
    pub border_color: Option<Color>,
    pub radius: Option<Space>,
}

impl Element {
    pub fn new(kind: Kind) -> Element {
        Element {
            kind,
            ..Element::default()
        }
    }

    pub fn pressable(&self) -> bool {
        self.pressable || matches!(self.kind, Kind::Button | Kind::Link)
    }

    pub fn focusable(&self) -> bool {
        self.focusable || matches!(self.kind, Kind::Button | Kind::Link | Kind::TextInput)
    }
}

// Token values are placeholders until the maintainer picks them.

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum Space {
    Xs,
    S,
    M,
    L,
    Xl,
    Px(f32),
}

impl Space {
    pub fn px(self) -> f32 {
        match self {
            Space::Xs => 2.0,
            Space::S => 4.0,
            Space::M => 8.0,
            Space::L => 16.0,
            Space::Xl => 32.0,
            Space::Px(px) => px,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum TextSize {
    S,
    M,
    L,
    Xl,
    Px(f32),
}

impl TextSize {
    pub fn px(self) -> f32 {
        match self {
            TextSize::S => 13.0,
            TextSize::M => 16.0,
            TextSize::L => 22.0,
            TextSize::Xl => 32.0,
            TextSize::Px(px) => px,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum Color {
    Background,
    Surface,
    Foreground,
    Muted,
    Accent,
    Clear,
    Rgb(u8, u8, u8),
}

impl Color {
    /// sRGB channels from 0 to 1, and alpha.
    pub fn rgba(self) -> [f32; 4] {
        let rgb = |r: u8, g: u8, b: u8| [r, g, b, 255].map(|c| c as f32 / 255.0);
        match self {
            Color::Background => rgb(24, 24, 28),
            Color::Surface => rgb(44, 44, 52),
            Color::Foreground => rgb(230, 230, 236),
            Color::Muted => rgb(150, 150, 160),
            Color::Accent => rgb(90, 140, 240),
            Color::Clear => [0.0; 4],
            Color::Rgb(r, g, b) => rgb(r, g, b),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum Length {
    Auto,
    Fill,
    Px(f32),
    Percent(f32),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum Direction {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum Justify {
    Start,
    Center,
    End,
    Between,
}
