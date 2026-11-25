use serde_json::{json, Value};
use veronica_protocol::manifest::{
    ActivationMode, Effect, HostPermission, Manifest, ManifestError, Profile, ShellService,
    SuggestSignal, SurfaceTemplate, Target, ValidationError, RESERVED_ROOTS,
};

const MEDIA: &str = include_str!("fixtures/media.json");

fn media() -> Value {
    serde_json::from_str(MEDIA).unwrap()
}

fn parse(value: &Value) -> Result<Manifest, ManifestError> {
    Manifest::parse(&value.to_string())
}

fn invalid(value: &Value) -> ValidationError {
    match parse(value) {
        Err(ManifestError::Invalid(error)) => error,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

fn rejected_by_parser(value: &Value) {
    assert!(
        matches!(parse(value), Err(ManifestError::Parse(_))),
        "expected a parse error"
    );
}

fn subcommands(value: &mut Value) -> &mut Vec<Value> {
    value["commands"][0]["subcommands"].as_array_mut().unwrap()
}

#[test]
fn media_fixture_parses_and_validates() {
    let manifest = Manifest::parse(MEDIA).unwrap();
    assert_eq!(manifest.id, "media");
    assert_eq!(manifest.version.to_string(), "1.0");
    assert_eq!(manifest.targets, [Target::LinuxX86_64]);
    assert_eq!(manifest.profiles, [Profile::Simple, Profile::Developer]);
    assert_eq!(manifest.activation.mode, ActivationMode::Background);
    assert_eq!(
        manifest.suggest_for[1],
        SuggestSignal::DesktopEntry("org.gnome.Rhythmbox3.desktop".to_string())
    );
    assert_eq!(manifest.home_cards[0].template, SurfaceTemplate::NowPlaying);
    assert_eq!(
        manifest.permissions.host,
        [HostPermission::Surfaces, HostPermission::Contracts]
    );
    assert_eq!(manifest.contracts.uses[0].to_string(), "host.presenter@1");
    let routes = manifest.leaf_routes();
    assert_eq!(routes.len(), 7);
    let (route, library) = &routes[2];
    assert_eq!(route, &["media", "library"]);
    assert_eq!(library.effect, Some(Effect::Read));
    assert_eq!(
        Manifest::mcp_tool_name(route, library),
        "veronica_media_library"
    );
}

#[test]
fn media_fixture_round_trips() {
    let manifest = Manifest::parse(MEDIA).unwrap();
    let written = serde_json::to_string(&manifest).unwrap();
    let reread = Manifest::parse(&written).unwrap();
    assert_eq!(reread, manifest);
    assert_eq!(serde_json::to_string(&reread).unwrap(), written);
}

#[test]
fn unknown_fields_are_rejected() {
    let mut value = media();
    value["surprise"] = json!(true);
    rejected_by_parser(&value);
    let mut value = media();
    subcommands(&mut value)[0]["confirm"] = json!(true);
    rejected_by_parser(&value);
}

#[test]
fn unsupported_schema_is_rejected() {
    let mut value = media();
    value["schema"] = json!(2);
    assert_eq!(invalid(&value), ValidationError::UnsupportedSchema(2));
}

#[test]
fn ids_must_be_short_kebab_case() {
    for id in [
        "m",
        "Media",
        "1media",
        "media_player",
        "-media",
        "media player",
        "a-very-long-extension-identifier-x",
    ] {
        let mut value = media();
        value["id"] = json!(id);
        assert_eq!(
            invalid(&value),
            ValidationError::InvalidId(id.to_string()),
            "{id}"
        );
    }
    let mut value = media();
    value["id"] = json!("ab");
    value["commands"][0]["name"] = json!("ab");
    value["contracts"]["provides"] = json!([]);
    parse(&value).unwrap();
}

#[test]
fn version_must_be_major_minor_with_an_optional_patch() {
    for version in ["1", "1.0.0.0", "v1.0", "01.0", "1.0.01"] {
        let mut value = media();
        value["version"] = json!(version);
        rejected_by_parser(&value);
    }
}

#[test]
fn packed_manifests_carry_the_stamped_patch() {
    let mut value = media();
    value["version"] = json!("1.0.7");
    let manifest = parse(&value).unwrap();
    assert_eq!(manifest.version.to_string(), "1.0.7");
    assert_eq!(manifest.version.authored().to_string(), "1.0");
}

#[test]
fn closed_enums_reject_unknown_values() {
    let cases = [
        ("/targets/0", json!("windows-x86_64")),
        ("/profiles/0", json!("creator")),
        ("/activation/mode", json!("always")),
        ("/homeCards/0/template", json!("chart")),
        ("/notchActivities/0/template", json!("map")),
        ("/commands/0/subcommands/0/effect", json!("delete")),
        ("/commands/0/subcommands/2/args/0/kind", json!("variadic")),
        ("/commands/0/subcommands/2/args/0/type", json!("date")),
        ("/permissions/host/0", json!("root")),
        ("/permissions/shell", json!(["keylogger"])),
        ("/permissions/filesystem/0/access", json!("execute")),
        ("/suggestFor/0", json!({ "process": "x" })),
        ("/contracts/provides/0/kind", json!("stream")),
    ];
    for (pointer, replacement) in cases {
        let mut value = media();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            matches!(parse(&value), Err(ManifestError::Parse(_))),
            "{pointer} should be rejected"
        );
    }
}

#[test]
fn every_shell_and_host_permission_parses() {
    let mut value = media();
    value["permissions"]["shell"] = json!([
        "clipboard.watch",
        "pasteInPlace",
        "pickColor",
        "focusDim",
        "keystrokeOverlay",
        "cleanKeys"
    ]);
    value["permissions"]["host"] = json!([
        "notifications",
        "surfaces",
        "contracts",
        "settings:read",
        "secrets",
        "openExternal",
        "clipboard:write",
        "shortcuts"
    ]);
    let manifest = parse(&value).unwrap();
    assert_eq!(manifest.permissions.shell[0], ShellService::ClipboardWatch);
    assert_eq!(manifest.permissions.host[3], HostPermission::SettingsRead);
    assert_eq!(manifest.permissions.host[6], HostPermission::ClipboardWrite);
}

#[test]
fn contract_ids_must_be_name_at_version() {
    for raw in [
        "media.nowPlaying",
        "media.nowPlaying@0",
        "Media@1",
        "media.now-playing@1",
    ] {
        let mut value = media();
        value["contracts"]["uses"] = json!([raw]);
        rejected_by_parser(&value);
        let mut value = media();
        value["homeCards"][0]["source"] = json!(raw);
        rejected_by_parser(&value);
    }
}

#[test]
fn provided_contracts_live_in_the_extension_namespace() {
    let mut value = media();
    value["contracts"]["provides"][0]["id"] = json!("music.nowPlaying@1");
    assert!(matches!(
        invalid(&value),
        ValidationError::ForeignContract { .. }
    ));
}

#[test]
fn reserved_host_roots_are_rejected() {
    for reserved in RESERVED_ROOTS {
        let mut value = media();
        value["commands"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "name": reserved, "effect": "read" }));
        assert_eq!(
            invalid(&value),
            ValidationError::ReservedRoot(reserved.to_string())
        );
        let mut value = media();
        value["commands"][0]["aliases"] = json!([reserved]);
        assert_eq!(
            invalid(&value),
            ValidationError::ReservedRoot(reserved.to_string())
        );
    }
}

#[test]
fn roots_and_aliases_are_unique() {
    let mut value = media();
    value["commands"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "music", "aliases": ["media"], "effect": "read" }));
    assert!(matches!(
        invalid(&value),
        ValidationError::Duplicate {
            field: "command root",
            ..
        }
    ));
    let mut value = media();
    value["commands"][0]["aliases"] = json!(["music", "music"]);
    assert!(matches!(
        invalid(&value),
        ValidationError::Duplicate {
            field: "command root",
            ..
        }
    ));
}

#[test]
fn subcommand_names_and_aliases_are_unique() {
    let mut value = media();
    subcommands(&mut value)[1]["aliases"] = json!(["prev"]);
    assert!(matches!(
        invalid(&value),
        ValidationError::Duplicate {
            field: "subcommand",
            ..
        }
    ));
}

#[test]
fn a_root_must_match_the_id() {
    let mut value = media();
    value["commands"][0]["name"] = json!("music");
    assert_eq!(
        invalid(&value),
        ValidationError::MissingPrimaryRoot("media".to_string())
    );
}

#[test]
fn leaves_must_declare_an_effect() {
    let mut value = media();
    subcommands(&mut value)[0]
        .as_object_mut()
        .unwrap()
        .remove("effect");
    assert_eq!(
        invalid(&value),
        ValidationError::MissingEffect("media status".to_string())
    );
}

#[test]
fn command_names_must_be_kebab_case() {
    let mut value = media();
    subcommands(&mut value)[0]["name"] = json!("Status");
    assert!(matches!(
        invalid(&value),
        ValidationError::InvalidName { .. }
    ));
}

#[test]
fn arguments_are_well_formed() {
    let mut value = media();
    subcommands(&mut value)[2]["args"][1]
        .as_object_mut()
        .unwrap()
        .remove("long");
    assert!(matches!(
        invalid(&value),
        ValidationError::InvalidArg { .. }
    ));
    let mut value = media();
    subcommands(&mut value)[2]["args"][0]["long"] = json!("query");
    assert!(matches!(
        invalid(&value),
        ValidationError::InvalidArg { .. }
    ));
    let mut value = media();
    subcommands(&mut value)[2]["args"][1]["name"] = json!("query");
    assert!(matches!(
        invalid(&value),
        ValidationError::Duplicate {
            field: "argument",
            ..
        }
    ));
}

#[test]
fn timeouts_must_be_positive() {
    let mut value = media();
    subcommands(&mut value)[0]["timeoutSeconds"] = json!(0);
    assert_eq!(
        invalid(&value),
        ValidationError::InvalidTimeout("media status".to_string())
    );
}

#[test]
fn mcp_tool_names_fit_the_limit_after_prefixing() {
    let fits = "a".repeat(64 - "veronica_".len());
    let mut value = media();
    subcommands(&mut value)[0]["mcp"] = json!({ "name": fits });
    parse(&value).unwrap();

    let mut value = media();
    subcommands(&mut value)[0]["mcp"] = json!({ "name": format!("{fits}b") });
    assert!(matches!(
        invalid(&value),
        ValidationError::McpNameTooLong { length: 65, .. }
    ));

    let long = format!("s{}", "-x".repeat(26));
    let mut value = media();
    subcommands(&mut value)[0]["name"] = json!(long);
    assert!(matches!(
        invalid(&value),
        ValidationError::McpNameTooLong { .. }
    ));
    subcommands(&mut value)[0]["mcp"] = json!({ "expose": false });
    parse(&value).unwrap();
}

#[test]
fn mcp_tool_names_are_unique_and_well_formed() {
    let mut value = media();
    subcommands(&mut value)[0]["mcp"] = json!({ "name": "media_players" });
    assert!(matches!(
        invalid(&value),
        ValidationError::Duplicate {
            field: "MCP tool",
            ..
        }
    ));
    let mut value = media();
    subcommands(&mut value)[0]["mcp"] = json!({ "name": "Media-Status" });
    assert!(matches!(
        invalid(&value),
        ValidationError::InvalidMcpName(_)
    ));
}

#[test]
fn surface_ids_are_unique_and_actions_run_something() {
    let mut value = media();
    let card = value["homeCards"][0].clone();
    value["homeCards"].as_array_mut().unwrap().push(card);
    assert!(matches!(
        invalid(&value),
        ValidationError::Duplicate {
            field: "home card id",
            ..
        }
    ));

    let mut value = media();
    value["notchActivities"][0]["actions"][0]["run"] = json!([]);
    assert!(matches!(invalid(&value), ValidationError::EmptyRun(_)));

    let mut value = media();
    value["shortcuts"][0]["run"] = json!([]);
    assert!(matches!(invalid(&value), ValidationError::EmptyRun(_)));

    let mut value = media();
    value["homeCards"][0]["opens"] = json!("settings");
    assert!(matches!(
        invalid(&value),
        ValidationError::UnknownPage { .. }
    ));
}

#[test]
fn targets_are_required_and_unique() {
    let mut value = media();
    value["targets"] = json!([]);
    assert_eq!(invalid(&value), ValidationError::NoTargets);
    let mut value = media();
    value["targets"] = json!(["linux-x86_64", "linux-x86_64"]);
    assert!(matches!(
        invalid(&value),
        ValidationError::Duplicate {
            field: "target",
            ..
        }
    ));
}

#[test]
fn engines_must_be_positive() {
    let mut value = media();
    value["engines"]["hostApi"] = json!(0);
    assert_eq!(invalid(&value), ValidationError::InvalidEngine("hostApi"));
}

#[test]
fn minimal_manifest_uses_defaults() {
    let manifest = Manifest::parse(
        &json!({
            "schema": 1,
            "id": "emoji",
            "version": "0.1",
            "title": "Emoji",
            "summary": "Pick an emoji.",
            "category": "desk",
            "engines": { "host": ">=0.2.0", "hostApi": 1 },
            "targets": ["linux-x86_64"]
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(manifest.activation.mode, ActivationMode::OnDemand);
    assert!(manifest.commands.is_empty());
    assert!(manifest.permissions.host.is_empty());
}

#[test]
fn heatmaps_belong_on_home_and_never_in_the_notch() {
    let mut value = media();
    value["homeCards"][0]["template"] = json!("heatmap");
    assert_eq!(
        parse(&value).unwrap().home_cards[0].template,
        SurfaceTemplate::Heatmap
    );
    value["notchActivities"][0]["template"] = json!("heatmap");
    assert_eq!(
        invalid(&value),
        ValidationError::NotchHeatmap("now-playing".to_string())
    );
}
