mod audit;
mod legacy;
mod media;

pub(crate) use audit::*;
pub(crate) use legacy::*;
pub(crate) use media::*;

type CommandResult<T> = Result<T, String>;

fn fail(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}
