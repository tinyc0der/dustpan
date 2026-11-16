use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyphs {
    pub sep: &'static str,
    pub admin: &'static str,
    pub broom: &'static str,
    pub bullet: &'static str,
    pub cursor: &'static str,
    pub ellipsis: &'static str,
    pub up: &'static str,
    pub down: &'static str,
    pub right: &'static str,
    pub left: &'static str,
    pub hline: &'static str,
    pub vline: &'static str,
    pub top_left: &'static str,
    pub top_right: &'static str,
    pub bottom_left: &'static str,
    pub bottom_right: &'static str,
}

impl Glyphs {
    pub const UNICODE: Glyphs = Glyphs {
        sep: "›",
        admin: "⚙ admin",
        broom: "🧹",
        bullet: "•",
        cursor: "➤",
        ellipsis: "…",
        up: "↑",
        down: "↓",
        right: "→",
        left: "←",
        hline: "─",
        vline: "│",
        top_left: "┌",
        top_right: "┐",
        bottom_left: "└",
        bottom_right: "┘",
    };

    pub const ASCII: Glyphs = Glyphs {
        sep: ">",
        admin: "* admin",
        broom: "~~",
        bullet: "-",
        cursor: ">",
        ellipsis: "~",
        up: "^",
        down: "v",
        right: ">",
        left: "<",
        hline: "-",
        vline: "|",
        top_left: "+",
        top_right: "+",
        bottom_left: "+",
        bottom_right: "+",
    };

    pub fn border(&self) -> ratatui::symbols::border::Set<'static> {
        ratatui::symbols::border::Set {
            top_left: self.top_left,
            top_right: self.top_right,
            bottom_left: self.bottom_left,
            bottom_right: self.bottom_right,
            vertical_left: self.vline,
            vertical_right: self.vline,
            horizontal_top: self.hline,
            horizontal_bottom: self.hline,
        }
    }

    #[cfg(test)]
    fn all(&self) -> [&'static str; 16] {
        [
            self.sep,
            self.admin,
            self.broom,
            self.bullet,
            self.cursor,
            self.ellipsis,
            self.up,
            self.down,
            self.right,
            self.left,
            self.hline,
            self.vline,
            self.top_left,
            self.top_right,
            self.bottom_left,
            self.bottom_right,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub glyphs: Glyphs,
    pub color: bool,
}

impl Look {
    pub const fn new(ascii: bool, color: bool) -> Self {
        Look {
            glyphs: if ascii {
                Glyphs::ASCII
            } else {
                Glyphs::UNICODE
            },
            color,
        }
    }

    pub fn selected(&self) -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }

    pub fn dim(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    pub fn bold(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    pub fn accent(&self) -> Style {
        if self.color {
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            self.bold()
        }
    }
}

pub fn wants_ascii(
    flag: bool,
    utf8_console: bool,
    locale: impl Fn(&str) -> Option<String>,
) -> bool {
    if flag {
        return true;
    }
    if utf8_console {
        return false;
    }
    let charset = ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|name| locale(name).filter(|value| !value.is_empty()));
    match charset {
        Some(value) => {
            let value = value.to_ascii_lowercase();
            !(value.contains("utf-8") || value.contains("utf8"))
        }
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn ascii_set_is_ascii_and_matches_every_cell_width() {
        for (unicode, ascii) in Glyphs::UNICODE.all().into_iter().zip(Glyphs::ASCII.all()) {
            assert!(ascii.is_ascii(), "{ascii:?} must be ASCII");
            assert_eq!(unicode.width(), ascii.width(), "{unicode} vs {ascii}");
        }
    }

    #[test]
    fn no_glyph_carries_the_emoji_variation_selector() {
        for glyph in Glyphs::UNICODE.all() {
            assert!(!glyph.contains('\u{FE0F}'), "{glyph:?}");
        }
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn locale_decides_ascii_on_a_console_that_follows_the_locale() {
        assert!(!wants_ascii(false, false, env(&[("LANG", "en_US.UTF-8")])));
        assert!(!wants_ascii(
            false,
            false,
            env(&[("LC_ALL", "C.utf8"), ("LANG", "C")])
        ));
        assert!(wants_ascii(
            false,
            false,
            env(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")])
        ));
        assert!(wants_ascii(false, false, env(&[])));
        assert!(wants_ascii(true, false, env(&[("LANG", "en_US.UTF-8")])));
        assert!(
            !wants_ascii(false, true, env(&[])),
            "a UTF-8 console ignores the locale"
        );
    }
}
