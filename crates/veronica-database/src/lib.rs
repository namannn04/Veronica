pub mod adapters;
pub mod capabilities;
pub mod connection;
pub mod guard;
pub mod identify;
pub mod mcp;
pub mod mutation;
pub mod paging;
pub mod product;
pub mod secrets;
pub mod session;
pub mod store;
pub mod value;

pub(crate) fn elapsed_millis(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
