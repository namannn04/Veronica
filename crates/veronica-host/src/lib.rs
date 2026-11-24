pub mod archive;
pub mod catalog;
pub mod connection;
pub mod fetch;
pub mod host_api;
pub mod install;
pub mod launcher;
pub mod layout;
pub mod lock;
pub mod mcp;
pub mod registry;
pub mod trust;
pub mod update;

pub use install::{HostIdentity, Installer};
pub use layout::Layout;
