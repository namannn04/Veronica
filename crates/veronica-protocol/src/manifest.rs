use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::contract_id::ContractId;
use crate::version::ManifestVersion;

pub use crate::validate::{ValidationError, MAX_MCP_TOOL_NAME, MCP_TOOL_PREFIX, RESERVED_ROOTS};

pub const MANIFEST_SCHEMA: u32 = 1;
pub const ID_PATTERN: &str = r"^[a-z][a-z0-9-]{1,31}$";
pub const COMMAND_NAME_PATTERN: &str = r"^[a-z][a-z0-9-]*$";
pub const MCP_NAME_PATTERN: &str = r"^[a-z][a-z0-9_]*$";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(title = "Veronica extension manifest")]
pub struct Manifest {
    pub schema: u32,
    #[schemars(pattern(ID_PATTERN))]
    pub id: String,
    pub version: ManifestVersion,
    pub title: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub category: String,
    #[serde(default)]
    pub labs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<String>,
    pub engines: Engines,
    pub targets: Vec<Target>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<Profile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggest_for: Vec<SuggestSignal>,
    #[serde(default)]
    pub activation: Activation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<Backend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<Ui>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub home_cards: Vec<HomeCard>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notch_activities: Vec<NotchActivity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<Command>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shortcuts: Vec<Shortcut>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub requires: Requirements,
    #[serde(default)]
    pub contracts: Contracts,
    #[serde(default)]
    pub data: Data,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Engines {
    pub host: String,
    #[schemars(range(min = 1))]
    pub host_api: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub backend_protocol: Option<u32>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum Target {
    #[serde(rename = "linux-x86_64")]
    LinuxX86_64,
    #[serde(rename = "linux-aarch64")]
    LinuxAarch64,
}

impl Target {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "linux-x86_64",
            Self::LinuxAarch64 => "linux-aarch64",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum Profile {
    Simple,
    Developer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum SuggestSignal {
    Tool(String),
    Path(String),
    DesktopEntry(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Activation {
    #[serde(default)]
    pub mode: ActivationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_stop_seconds: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ActivationMode {
    #[default]
    OnDemand,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Backend {
    pub entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ui {
    pub root: String,
    pub pages: Vec<UiPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPage {
    pub id: String,
    pub title: String,
    pub entry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nav: Option<Nav>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Nav {
    pub group: String,
    #[serde(default)]
    pub order: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceTemplate {
    Ring,
    Meter,
    Stat,
    Text,
    List,
    NowPlaying,
    Actions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceAction {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub run: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HomeCard {
    pub id: String,
    pub template: SurfaceTemplate,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ContractId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SurfaceAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotchActivity {
    pub id: String,
    pub template: SurfaceTemplate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ContractId>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur_category: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SurfaceAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Command {
    #[schemars(pattern(COMMAND_NAME_PATTERN))]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_about: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<Effect>,
    #[serde(default)]
    pub json: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<Arg>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub stdin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub timeout_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpOptions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subcommands: Vec<Self>,
}

impl Command {
    pub fn is_leaf(&self) -> bool {
        self.subcommands.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.name.as_str()).chain(self.aliases.iter().map(String::as_str))
    }

    pub fn mcp_exposed(&self) -> bool {
        self.mcp.as_ref().is_none_or(|mcp| mcp.expose)
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes the field by reference"
)]
fn is_false(value: &bool) -> bool {
    !*value
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes the field by reference"
)]
fn is_true(value: &bool) -> bool {
    *value
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    Read,
    Write,
    Destructive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Arg {
    pub name: String,
    pub kind: ArgKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<char>,
    #[serde(rename = "type")]
    pub value_type: ArgType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ArgKind {
    Positional,
    Option,
    Flag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ArgType {
    String,
    Integer,
    Number,
    Boolean,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpOptions {
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub expose: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(pattern(MCP_NAME_PATTERN))]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shortcut {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub default: Option<String>,
    pub run: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub filesystem: Vec<FilesystemGrant>,
    #[serde(default)]
    pub dbus: DbusGrants,
    #[serde(default)]
    pub spawn: Vec<String>,
    #[serde(default)]
    pub shell: Vec<ShellService>,
    #[serde(default)]
    pub host: Vec<HostPermission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilesystemGrant {
    pub path: String,
    pub access: FilesystemAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FilesystemAccess {
    Read,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DbusGrants {
    #[serde(default)]
    pub session: Vec<String>,
    #[serde(default)]
    pub system: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum ShellService {
    #[serde(rename = "clipboard.watch")]
    ClipboardWatch,
    #[serde(rename = "pasteInPlace")]
    PasteInPlace,
    #[serde(rename = "pickColor")]
    PickColor,
    #[serde(rename = "focusDim")]
    FocusDim,
    #[serde(rename = "keystrokeOverlay")]
    KeystrokeOverlay,
    #[serde(rename = "cleanKeys")]
    CleanKeys,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum HostPermission {
    #[serde(rename = "notifications")]
    Notifications,
    #[serde(rename = "surfaces")]
    Surfaces,
    #[serde(rename = "contracts")]
    Contracts,
    #[serde(rename = "settings:read")]
    SettingsRead,
    #[serde(rename = "secrets")]
    Secrets,
    #[serde(rename = "openExternal")]
    OpenExternal,
    #[serde(rename = "clipboard:write")]
    ClipboardWrite,
    #[serde(rename = "shortcuts")]
    Shortcuts,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Requirements {
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub optional_capabilities: Vec<String>,
    #[serde(default)]
    pub system_packages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contracts {
    #[serde(default)]
    pub provides: Vec<ProvidedContract>,
    #[serde(default)]
    pub uses: Vec<ContractId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvidedContract {
    pub id: ContractId,
    pub kind: ContractKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ContractKind {
    Topic,
    Request,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Data {
    #[serde(default)]
    pub migrate: Vec<Migration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Migration {
    pub from: String,
    pub to: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest is not valid JSON for this schema: {0}")]
    Parse(#[from] serde_json::Error),
    #[error(transparent)]
    Invalid(#[from] ValidationError),
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn roots(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter()
    }

    pub fn leaf_routes(&self) -> Vec<(Vec<&str>, &Command)> {
        let mut routes = Vec::new();
        for root in &self.commands {
            collect_leaves(root, &mut Vec::new(), &mut routes);
        }
        routes
    }

    pub fn mcp_tool_name(route: &[&str], command: &Command) -> String {
        let suffix = command
            .mcp
            .as_ref()
            .and_then(|mcp| mcp.name.clone())
            .unwrap_or_else(|| route.join("_").replace('-', "_"));
        format!("{MCP_TOOL_PREFIX}{suffix}")
    }
}

fn collect_leaves<'a>(
    command: &'a Command,
    path: &mut Vec<&'a str>,
    routes: &mut Vec<(Vec<&'a str>, &'a Command)>,
) {
    path.push(&command.name);
    if command.is_leaf() {
        routes.push((path.clone(), command));
    }
    for child in &command.subcommands {
        collect_leaves(child, path, routes);
    }
    path.pop();
}

pub fn json_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Manifest))
        .expect("a generated schema always serializes")
}
