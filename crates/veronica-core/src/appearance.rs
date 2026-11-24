#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Light,
    Dark,
    System,
}

impl Scheme {
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::Light => Some("light"),
            Self::Dark => Some("dark"),
            Self::System => None,
        }
    }
}

pub const LIGHT_THEMES: [&str; 4] = ["light", "sandstone", "mist", "paper"];

pub const DARK_THEMES: [&str; 8] = [
    "dark",
    "midnight",
    "aubergine",
    "forest",
    "nord",
    "ocean",
    "ember",
    "carbon",
];

pub fn scheme(appearance: &str) -> Scheme {
    if LIGHT_THEMES.contains(&appearance) {
        Scheme::Light
    } else if DARK_THEMES.contains(&appearance) {
        Scheme::Dark
    } else {
        Scheme::System
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_families_do_not_overlap() {
        for theme in LIGHT_THEMES {
            assert!(!DARK_THEMES.contains(&theme), "{theme} is in both families");
        }
    }

    #[test]
    fn a_dark_theme_that_is_not_called_dark_still_resolves_to_dark() {
        assert_eq!(scheme("midnight"), Scheme::Dark);
        assert_eq!(scheme("carbon"), Scheme::Dark);
        assert_eq!(scheme("sandstone"), Scheme::Light);
        assert_eq!(scheme("paper"), Scheme::Light);
    }

    #[test]
    fn system_and_anything_unknown_defer_to_the_desktop() {
        assert_eq!(scheme("system"), Scheme::System);
        assert_eq!(scheme("solarized-from-the-future"), Scheme::System);
        assert_eq!(scheme("system").name(), None);
    }

    #[test]
    fn the_names_are_what_a_terminal_expects() {
        assert_eq!(scheme("light").name(), Some("light"));
        assert_eq!(scheme("forest").name(), Some("dark"));
    }
}
