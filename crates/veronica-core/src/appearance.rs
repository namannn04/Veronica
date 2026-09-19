//! Which colour scheme an appearance actually is.
//!
//! The `appearance` setting names a theme, such as `midnight`, `sandstone` or
//! `carbon`, and every one of them is either light or dark underneath.
//! Anything Veronica launches with a theme of its own, Quinjet's review TUI
//! for one, needs that answer rather than the theme's name, so it does not
//! open a light terminal inside a dark desktop.
//!
//! This is the Rust half of the catalogue in
//! `apps/desktop/src/lib/preferences.ts`; the two lists have to agree. A theme
//! missing from both lists below resolves to [`Scheme::System`], which is the
//! safe answer: it means "ask the desktop", not "assume light".

/// The colour scheme behind an appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Light,
    Dark,
    /// `system`, or a theme this build does not know: defer to the desktop.
    System,
}

impl Scheme {
    /// The name to hand a program that takes `light` or `dark`, or `None` when
    /// it should be left to work the answer out itself.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Scheme::Light => Some("light"),
            Scheme::Dark => Some("dark"),
            Scheme::System => None,
        }
    }
}

/// Appearances that paint a light island and light app chrome.
pub const LIGHT_THEMES: [&str; 4] = ["light", "sandstone", "mist", "paper"];

/// Appearances that paint a dark one.
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

/// Resolve an `appearance` setting to the scheme behind it.
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
        // The bug this guards: only `light` and `dark` used to be recognised,
        // so choosing Midnight opened Quinjet in whatever the terminal
        // defaulted to - light, inside an unmistakably dark desktop.
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
