use std::collections::BTreeSet;

use crate::manifest::{
    ArgKind, Command, Manifest, SurfaceAction, SurfaceTemplate, MANIFEST_SCHEMA,
};

pub const RESERVED_ROOTS: [&str; 11] = [
    "extensions",
    "guide",
    "invoke",
    "surfaces",
    "mcp",
    "diagnose",
    "config",
    "backup",
    "presenter",
    "tools",
    "help",
];
pub const MCP_TOOL_PREFIX: &str = "veronica_";
pub const MAX_MCP_TOOL_NAME: usize = 64;
const MAX_ID_LENGTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("manifest schema {0} is not supported")]
    UnsupportedSchema(u32),
    #[error("extension id {0:?} must match ^[a-z][a-z0-9-]{{1,31}}$")]
    InvalidId(String),
    #[error("{field} {value:?} must be lowercase kebab-case")]
    InvalidName { field: &'static str, value: String },
    #[error("engines.{0} must be at least 1")]
    InvalidEngine(&'static str),
    #[error("targets must list at least one target")]
    NoTargets,
    #[error("{field} {value:?} appears more than once")]
    Duplicate { field: &'static str, value: String },
    #[error("command root {0:?} is reserved by the host")]
    ReservedRoot(String),
    #[error("no command root is named after the extension id {0:?}")]
    MissingPrimaryRoot(String),
    #[error("command {0:?} has no subcommands and must declare an effect")]
    MissingEffect(String),
    #[error("argument {arg:?} of {command:?}: {reason}")]
    InvalidArg {
        command: String,
        arg: String,
        reason: &'static str,
    },
    #[error("timeoutSeconds of {0:?} must be at least 1")]
    InvalidTimeout(String),
    #[error("MCP tool name {0:?} must match ^[a-z][a-z0-9_]*$")]
    InvalidMcpName(String),
    #[error(
        "MCP tool name {name:?} is {length} characters, over the limit of {MAX_MCP_TOOL_NAME}"
    )]
    McpNameTooLong { name: String, length: usize },
    #[error("{0} must name a command to run")]
    EmptyRun(String),
    #[error("home card {card:?} opens unknown page {page:?}")]
    UnknownPage { card: String, page: String },
    #[error("notch activity {0:?} cannot use the heatmap template")]
    NotchHeatmap(String),
    #[error("provided contract {contract} must live in the {id:?} namespace")]
    ForeignContract { contract: String, id: String },
}

fn is_kebab(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_extension_id(value: &str) -> bool {
    (2..=MAX_ID_LENGTH).contains(&value.len()) && is_kebab(value)
}

fn is_mcp_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn kebab(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if is_kebab(value) {
        Ok(())
    } else {
        Err(ValidationError::InvalidName {
            field,
            value: value.to_string(),
        })
    }
}

fn unique<'a>(
    field: &'static str,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), ValidationError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ValidationError::Duplicate {
                field,
                value: value.to_string(),
            });
        }
    }
    Ok(())
}

impl Manifest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema != MANIFEST_SCHEMA {
            return Err(ValidationError::UnsupportedSchema(self.schema));
        }
        if !is_extension_id(&self.id) {
            return Err(ValidationError::InvalidId(self.id.clone()));
        }
        kebab("category", &self.category)?;
        if self.engines.host_api == 0 {
            return Err(ValidationError::InvalidEngine("hostApi"));
        }
        if self.engines.backend_protocol == Some(0) {
            return Err(ValidationError::InvalidEngine("backendProtocol"));
        }
        if self.targets.is_empty() {
            return Err(ValidationError::NoTargets);
        }
        unique("target", self.targets.iter().map(|target| target.as_str()))?;
        self.validate_commands()?;
        self.validate_surfaces()?;
        self.validate_contracts()
    }

    fn validate_commands(&self) -> Result<(), ValidationError> {
        for root in &self.commands {
            for name in root.names() {
                if RESERVED_ROOTS.contains(&name) {
                    return Err(ValidationError::ReservedRoot(name.to_string()));
                }
            }
        }
        unique(
            "command root",
            self.commands.iter().flat_map(Command::names),
        )?;
        if !self.commands.is_empty() && !self.commands.iter().any(|root| root.name == self.id) {
            return Err(ValidationError::MissingPrimaryRoot(self.id.clone()));
        }
        for root in &self.commands {
            validate_command(root, &root.name)?;
        }
        let mut tools = BTreeSet::new();
        for (route, command) in self.leaf_routes() {
            if !command.mcp_exposed() {
                continue;
            }
            if let Some(name) = command.mcp.as_ref().and_then(|mcp| mcp.name.as_deref()) {
                if !is_mcp_name(name) {
                    return Err(ValidationError::InvalidMcpName(name.to_string()));
                }
            }
            let tool = Self::mcp_tool_name(&route, command);
            if tool.len() > MAX_MCP_TOOL_NAME {
                return Err(ValidationError::McpNameTooLong {
                    length: tool.len(),
                    name: tool,
                });
            }
            if !tools.insert(tool.clone()) {
                return Err(ValidationError::Duplicate {
                    field: "MCP tool",
                    value: tool,
                });
            }
        }
        Ok(())
    }

    fn validate_surfaces(&self) -> Result<(), ValidationError> {
        let pages = self
            .ui
            .as_ref()
            .map(|ui| ui.pages.as_slice())
            .unwrap_or_default();
        unique("page id", pages.iter().map(|page| page.id.as_str()))?;
        unique(
            "home card id",
            self.home_cards.iter().map(|card| card.id.as_str()),
        )?;
        unique(
            "notch activity id",
            self.notch_activities
                .iter()
                .map(|activity| activity.id.as_str()),
        )?;
        unique(
            "shortcut id",
            self.shortcuts.iter().map(|shortcut| shortcut.id.as_str()),
        )?;
        for card in &self.home_cards {
            if let Some(page) = &card.opens {
                if !pages.iter().any(|candidate| &candidate.id == page) {
                    return Err(ValidationError::UnknownPage {
                        card: card.id.clone(),
                        page: page.clone(),
                    });
                }
            }
            validate_actions(&format!("home card {}", card.id), &card.actions)?;
        }
        for activity in &self.notch_activities {
            if activity.template == SurfaceTemplate::Heatmap {
                return Err(ValidationError::NotchHeatmap(activity.id.clone()));
            }
            validate_actions(
                &format!("notch activity {}", activity.id),
                &activity.actions,
            )?;
        }
        for shortcut in &self.shortcuts {
            if shortcut.run.is_empty() {
                return Err(ValidationError::EmptyRun(format!(
                    "shortcut {}",
                    shortcut.id
                )));
            }
        }
        Ok(())
    }

    fn validate_contracts(&self) -> Result<(), ValidationError> {
        for provided in &self.contracts.provides {
            if provided.id.namespace() != self.id {
                return Err(ValidationError::ForeignContract {
                    contract: provided.id.to_string(),
                    id: self.id.clone(),
                });
            }
        }
        let provided: Vec<String> = self
            .contracts
            .provides
            .iter()
            .map(|provided| provided.id.to_string())
            .collect();
        unique("provided contract", provided.iter().map(String::as_str))?;
        let used: Vec<String> = self
            .contracts
            .uses
            .iter()
            .map(ToString::to_string)
            .collect();
        unique("used contract", used.iter().map(String::as_str))
    }
}

fn validate_actions(owner: &str, actions: &[SurfaceAction]) -> Result<(), ValidationError> {
    unique(
        "surface action id",
        actions.iter().map(|action| action.id.as_str()),
    )?;
    for action in actions {
        if action.run.is_empty() {
            return Err(ValidationError::EmptyRun(format!(
                "{owner} action {}",
                action.id
            )));
        }
    }
    Ok(())
}

fn validate_command(command: &Command, path: &str) -> Result<(), ValidationError> {
    for name in command.names() {
        kebab("command name", name)?;
    }
    if command.is_leaf() && command.effect.is_none() {
        return Err(ValidationError::MissingEffect(path.to_string()));
    }
    if command.timeout_seconds == Some(0) {
        return Err(ValidationError::InvalidTimeout(path.to_string()));
    }
    unique("argument", command.args.iter().map(|arg| arg.name.as_str()))?;
    for arg in &command.args {
        let invalid = |reason| ValidationError::InvalidArg {
            command: path.to_string(),
            arg: arg.name.clone(),
            reason,
        };
        match arg.kind {
            ArgKind::Positional if arg.long.is_some() || arg.short.is_some() => {
                return Err(invalid("a positional argument takes no long or short flag"));
            }
            ArgKind::Option | ArgKind::Flag if arg.long.is_none() => {
                return Err(invalid("an option or flag needs a long name"));
            }
            _ => {}
        }
        if let Some(long) = &arg.long {
            kebab("long flag", long)?;
        }
    }
    unique(
        "long flag",
        command.args.iter().filter_map(|arg| arg.long.as_deref()),
    )?;
    unique(
        "subcommand",
        command.subcommands.iter().flat_map(Command::names),
    )?;
    for child in &command.subcommands {
        validate_command(child, &format!("{path} {}", child.name))?;
    }
    Ok(())
}
