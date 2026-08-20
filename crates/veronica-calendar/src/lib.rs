//! Calendar support for Veronica.
//!
//! The agenda comes from GNOME's calendar server, which aggregates every
//! configured calendar and expands recurrences. It does not pass the location or
//! description through, so join links are fetched per event from Evolution Data
//! Server. The grouping and labelling logic is pure and lives in `agenda`, so it
//! is tested without a session bus.

pub mod agenda;
pub mod ical;
pub mod links;
pub mod server;

pub use agenda::{AgendaDay, Event};
pub use server::{events, events_with_links, has_calendars};
