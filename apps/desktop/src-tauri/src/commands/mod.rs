mod attention;
mod audit;
mod calendar;
mod clipboard;
mod emoji;
mod legacy;
mod media;

pub(crate) use attention::*;
pub(crate) use audit::*;
pub(crate) use calendar::*;
pub(crate) use clipboard::*;
pub(crate) use emoji::*;
pub(crate) use legacy::*;
pub(crate) use media::*;

type CommandResult<T> = Result<T, String>;

fn fail(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}
