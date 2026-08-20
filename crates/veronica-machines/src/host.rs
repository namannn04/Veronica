//! The machines Veronica can reach.
//!
//! Edith keeps a fleet of the local Mac plus SSH hosts. Veronica does the same:
//! "this computer" is always present and needs no configuration, and remote
//! hosts are read from Veronica's settings, seeded from `~/.ssh/config` so a
//! machine already reachable over SSH does not have to be described twice.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// How to reach a machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Reach {
    /// The machine Veronica is running on. Probed directly, no SSH.
    Local,
    /// Reached by running `ssh <target>`, so the user's own SSH configuration,
    /// keys and agent apply unchanged.
    Ssh {
        /// The argument passed to `ssh`: a config alias, or `user@host`.
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port: Option<u16>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    /// Stable identifier used in settings and on the command line.
    pub id: String,
    /// What to call it in the interface.
    pub name: String,
    pub reach: Reach,
}

impl Machine {
    /// The entry that always exists.
    pub fn local() -> Self {
        Self {
            id: "local".to_string(),
            name: "This computer".to_string(),
            reach: Reach::Local,
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self.reach, Reach::Local)
    }

    /// The `ssh` target, for a remote machine.
    pub fn ssh_target(&self) -> Option<&str> {
        match &self.reach {
            Reach::Ssh { target, .. } => Some(target),
            Reach::Local => None,
        }
    }
}

/// Turn a name into an identifier: lowercase, with runs of anything unusual
/// collapsed to single hyphens, so it is safe in settings keys and on a
/// command line.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_hyphen = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_hyphen = true;
        }
    }
    if out.is_empty() {
        "machine".to_string()
    } else {
        out
    }
}

/// Host aliases from an SSH config file.
///
/// Only plain `Host` aliases are offered: a pattern with a wildcard is a rule
/// for many hosts rather than one machine, and `Host *` in particular would
/// otherwise appear as a machine called "*".
pub fn parse_ssh_config(contents: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case("Host") {
            continue;
        }
        for alias in parts {
            if alias.contains('*') || alias.contains('?') || alias.contains('!') {
                continue;
            }
            if !hosts.iter().any(|existing| existing == alias) {
                hosts.push(alias.to_string());
            }
        }
    }
    hosts
}

/// Read host aliases from the user's SSH config, if it has one.
pub fn ssh_config_hosts(path: &Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => parse_ssh_config(&contents),
        Err(_) => Vec::new(),
    }
}

/// The stored fleet, always led by this computer.
///
/// Deduplicated by id, and the local entry is never displaced by a stored one,
/// so a bad settings file cannot remove the machine the user is sitting at.
pub fn fleet(stored: Vec<Machine>) -> Vec<Machine> {
    let mut machines = vec![Machine::local()];
    for machine in stored {
        if machine.id == "local" || machine.is_local() {
            continue;
        }
        if machines.iter().any(|existing| existing.id == machine.id) {
            continue;
        }
        machines.push(machine);
    }
    machines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_local_machine_needs_no_configuration() {
        let local = Machine::local();
        assert!(local.is_local());
        assert_eq!(local.ssh_target(), None);
        assert_eq!(local.id, "local");
    }

    #[test]
    fn slugify_makes_safe_identifiers() {
        assert_eq!(slugify("Build Server"), "build-server");
        assert_eq!(slugify("nas.local"), "nas-local");
        assert_eq!(slugify("  Tuf   Laptop  "), "tuf-laptop");
        assert_eq!(slugify("user@10.0.0.5"), "user-10-0-0-5");
    }

    #[test]
    fn slugify_never_returns_an_empty_or_edge_hyphenated_id() {
        assert_eq!(slugify(""), "machine");
        assert_eq!(slugify("!!!"), "machine");
        assert_eq!(slugify("-x-"), "x");
    }

    #[test]
    fn reads_plain_host_aliases_from_ssh_config() {
        let config = "\
# a comment
Host tuf
  HostName 10.0.0.5
  User naman

Host nas backup-nas
  HostName nas.local
";
        assert_eq!(parse_ssh_config(config), vec!["tuf", "nas", "backup-nas"]);
    }

    #[test]
    fn skips_wildcard_patterns_which_are_rules_not_machines() {
        let config = "\
Host *
  ServerAliveInterval 60
Host *.example.com
  User deploy
Host build?
  User ci
Host real-host
  HostName 10.0.0.9
";
        assert_eq!(parse_ssh_config(config), vec!["real-host"]);
    }

    #[test]
    fn host_keyword_matching_is_case_insensitive_and_deduplicated() {
        let config = "host alpha\nHOST alpha\nHost beta\n";
        assert_eq!(parse_ssh_config(config), vec!["alpha", "beta"]);
    }

    #[test]
    fn an_absent_ssh_config_yields_no_hosts() {
        assert!(ssh_config_hosts(Path::new("/nonexistent/ssh/config")).is_empty());
    }

    #[test]
    fn the_fleet_always_leads_with_this_computer() {
        let stored = vec![Machine {
            id: "tuf".into(),
            name: "Tuf".into(),
            reach: Reach::Ssh { target: "tuf".into(), port: None },
        }];
        let fleet = fleet(stored);
        assert_eq!(fleet.len(), 2);
        assert!(fleet[0].is_local());
        assert_eq!(fleet[1].id, "tuf");
    }

    #[test]
    fn a_stored_entry_cannot_displace_or_duplicate_the_local_machine() {
        let stored = vec![
            Machine { id: "local".into(), name: "Impostor".into(), reach: Reach::Local },
            Machine {
                id: "tuf".into(),
                name: "Tuf".into(),
                reach: Reach::Ssh { target: "tuf".into(), port: None },
            },
            Machine {
                id: "tuf".into(),
                name: "Tuf again".into(),
                reach: Reach::Ssh { target: "other".into(), port: None },
            },
        ];
        let fleet = fleet(stored);
        assert_eq!(fleet.len(), 2, "the impostor and the duplicate are dropped");
        assert_eq!(fleet[0].name, "This computer");
        assert_eq!(fleet[1].name, "Tuf");
    }
}
