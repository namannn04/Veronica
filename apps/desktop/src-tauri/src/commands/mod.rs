mod audit;
mod legacy;

pub(crate) use audit::*;
pub(crate) use legacy::*;

type CommandResult<T> = Result<T, String>;

fn fail(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}
