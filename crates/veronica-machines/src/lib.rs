//! The machines Veronica can reach.
//!
//! "This computer" is always present. Remote hosts are reached by running the
//! `ssh` binary, so the user's existing SSH configuration applies unchanged and
//! Veronica never handles a key or a passphrase. Both are probed with the same
//! shell snippet and parsed by the same code, so a remote machine reports
//! exactly what a local one does.

pub mod host;
pub mod probe;
pub mod transport;

pub use host::{fleet, Machine, Reach};
pub use probe::{DiskUsage, MachineStats};
pub use transport::{probe_fleet, probe_machine, MachineReport, DEFAULT_TIMEOUT};
