use bwapi_sys as sys;

#[derive(PartialEq, PartialOrd, Copy, Clone)]
pub enum Color {
    Black = 0,
    Brown = 19,
    Grey = 74,
    Red = 111,
    Green = 117,
    Cyan = 128,
    Yellow = 135,
    Teal = 159,
    Purple = 164,
    Blue = 165,
    Orange = 179,
    White = 255,
}

impl From<sys::Color> for Color {
    fn from(input: sys::Color) -> Color {
        match input.color {
            0 => Color::Black,
            19 => Color::Brown,
            74 => Color::Grey,
            111 => Color::Red,
            117 => Color::Green,
            128 => Color::Cyan,
            135 => Color::Yellow,
            159 => Color::Teal,
            164 => Color::Purple,
            165 => Color::Blue,
            179 => Color::Orange,
            255 => Color::White,
            _ => panic!("No such color in enum: {}", input.color),
        }
    }
}

impl From<Color> for sys::Color {
    fn from(input: Color) -> sys::Color {
        use std::os::raw::c_int;
        sys::Color { color: input as c_int }
    }
}
