use std::fmt::Write;

use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::connection::{
    ConnectionDefinition, EnvironmentKind, EnvironmentProtection, Prohibition,
};
use crate::identify::TargetIdentifier;
use crate::mutation::{
    ConfirmationStrength, Effect, MutationPreview, Plan, RequiredConfirmation, Scope,
    TransactionBehavior, Warning,
};

type HmacSha256 = Hmac<Sha256>;

pub const DEFAULT_LIFETIME_SECONDS: i64 = 120;
pub const MIN_LIFETIME_SECONDS: i64 = 5;
pub const MAX_LIFETIME_SECONDS: i64 = 900;

pub const SIGNING_KEY_BYTES: usize = 32;

pub const MAX_TOKEN_BYTES: usize = 4_096;

const TOKEN_SCHEMA_VERSION: u32 = 1;
const TOKEN_AUDIENCE: &str = "veronica.database.confirmation";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GuardError {
    #[error("the signing key must be {SIGNING_KEY_BYTES} bytes, not {0}")]
    InvalidSigningKey(usize),
    #[error("a preview lifetime must be between {MIN_LIFETIME_SECONDS} and {MAX_LIFETIME_SECONDS} seconds, not {0}")]
    InvalidLifetime(i64),
    #[error("mutations are not allowed on this connection: {}", .0.reason())]
    Prohibited(Prohibition),
    #[error("this plan targets {expected:?} but the payload is for {actual:?}")]
    ProductMismatch {
        expected: crate::product::Product,
        actual: crate::product::Product,
    },
    #[error("the payload does not match the product's family")]
    FamilyMismatch,
    #[error("this plan is not one Veronica can act on: {0}")]
    InvalidPlan(String),
    #[error("that is not a token Veronica issued")]
    MalformedToken,
    #[error("that token's signature does not match")]
    InvalidSignature,
    #[error("that preview has expired; take a new one")]
    Expired,
    #[error("that preview has already been used; take a new one")]
    AlreadyUsed,
    #[error("this token was issued for a different change, so nothing was run")]
    PlanChanged,
    #[error("the confirmation text does not match")]
    ConfirmationMismatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub effect: Effect,
    pub request: MutationPreview,
    pub warnings: Vec<Warning>,
    pub required_confirmation: RequiredConfirmation,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TokenPayload {
    version: u32,
    audience: String,
    identifier: Uuid,
    execution_digest: String,
    display_digest: String,
    issued_at_millis: i64,
    expires_at_millis: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub identifier: Uuid,
    pub effect_digest: String,
    pub expires_at_millis: i64,
}

pub trait ReceiptStore {
    fn register(&mut self, receipt: Receipt) -> anyhow::Result<()>;
    fn consume(&mut self, identifier: Uuid) -> anyhow::Result<Option<Receipt>>;
    fn purge_expired(&mut self, now_millis: i64) -> anyhow::Result<usize>;
}

impl<S: ReceiptStore + ?Sized> ReceiptStore for &mut S {
    fn register(&mut self, receipt: Receipt) -> anyhow::Result<()> {
        (**self).register(receipt)
    }

    fn consume(&mut self, identifier: Uuid) -> anyhow::Result<Option<Receipt>> {
        (**self).consume(identifier)
    }

    fn purge_expired(&mut self, now_millis: i64) -> anyhow::Result<usize> {
        (**self).purge_expired(now_millis)
    }
}

#[derive(Debug, Default)]
pub struct MemoryReceiptStore {
    receipts: std::collections::HashMap<Uuid, Receipt>,
}

impl ReceiptStore for MemoryReceiptStore {
    fn register(&mut self, receipt: Receipt) -> anyhow::Result<()> {
        if self.receipts.contains_key(&receipt.identifier) {
            anyhow::bail!("a confirmation with that identifier already exists");
        }
        self.receipts.insert(receipt.identifier, receipt);
        Ok(())
    }

    fn consume(&mut self, identifier: Uuid) -> anyhow::Result<Option<Receipt>> {
        Ok(self.receipts.remove(&identifier))
    }

    fn purge_expired(&mut self, now_millis: i64) -> anyhow::Result<usize> {
        let before = self.receipts.len();
        self.receipts
            .retain(|_, receipt| receipt.expires_at_millis > now_millis);
        Ok(before - self.receipts.len())
    }
}

fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn from_base64url(text: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text)
        .ok()
}

fn canonical<T: Serialize>(value: &T) -> anyhow::Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(Into::into)
}

fn domain_separated(data: &[u8], domain: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(domain.len() + data.len() + 9);
    out.extend_from_slice(&(domain.len() as u64).to_be_bytes());
    out.extend_from_slice(domain.as_bytes());
    out.push(0);
    out.extend_from_slice(data);
    out
}

pub struct Guard<S: ReceiptStore> {
    signing_key: Vec<u8>,
    receipts: S,
}

impl<S: ReceiptStore> std::fmt::Debug for Guard<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard")
            .field("signing_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl<S: ReceiptStore> Guard<S> {
    pub fn new(signing_key: Vec<u8>, receipts: S) -> Result<Self, GuardError> {
        if signing_key.len() != SIGNING_KEY_BYTES {
            return Err(GuardError::InvalidSigningKey(signing_key.len()));
        }
        Ok(Self {
            signing_key,
            receipts,
        })
    }

    fn mac(&self, data: &[u8], domain: &str) -> Vec<u8> {
        let mut mac =
            HmacSha256::new_from_slice(&self.signing_key).expect("HMAC takes a key of any size");
        mac.update(&domain_separated(data, domain));
        mac.finalize().into_bytes().to_vec()
    }

    fn digest(&self, data: &[u8], domain: &str) -> String {
        self.mac(data, domain)
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    fn validate(plan: &Plan, connection: &ConnectionDefinition) -> Result<(), GuardError> {
        if let Some(prohibition) = connection.mutation_prohibition() {
            return Err(GuardError::Prohibited(prohibition));
        }
        if plan.payload.product() != connection.product_hint {
            return Err(GuardError::ProductMismatch {
                expected: connection.product_hint,
                actual: plan.payload.product(),
            });
        }
        if !plan.payload.is_family_consistent() {
            return Err(GuardError::FamilyMismatch);
        }
        if plan.payload.command().trim().is_empty() {
            return Err(GuardError::InvalidPlan(
                "the payload has no statement or command".into(),
            ));
        }
        if plan.target.connection_id != connection.id {
            return Err(GuardError::InvalidPlan(
                "the plan targets a different connection".into(),
            ));
        }
        if plan.scope == Scope::Predicate && plan.predicate.is_none() {
            return Err(GuardError::InvalidPlan(
                "the plan says it is bounded by a predicate but carries none".into(),
            ));
        }
        if plan.scope == Scope::SelectedRecords && plan.selected_records.is_empty() {
            return Err(GuardError::InvalidPlan(
                "the plan says it targets selected records but names none".into(),
            ));
        }
        if let Some(filter) = &plan.predicate {
            if !filter.is_well_formed() {
                return Err(GuardError::InvalidPlan("the predicate is malformed".into()));
            }
        }
        Ok(())
    }

    pub fn required_confirmation(
        plan: &Plan,
        connection: &ConnectionDefinition,
    ) -> RequiredConfirmation {
        let target_text = render_target(&plan.target);
        let strongest = connection.environment.kind == EnvironmentKind::Production
            || connection.environment.protection == EnvironmentProtection::ConfirmationRequired
            || plan.scope == Scope::EntireObject
            || plan.transaction_behavior != TransactionBehavior::Transactional
            || plan.rollback_availability != crate::mutation::RollbackAvailability::Available
            || plan.execution_mode == crate::mutation::ExecutionMode::Asynchronous
            || plan.action.is_strongest();

        if strongest {
            RequiredConfirmation {
                strength: ConfirmationStrength::ConnectionAndTarget,
                text: format!(
                    "connection[{}] target[{target_text}]",
                    length_prefixed(&connection.display_name)
                ),
            }
        } else if matches!(plan.scope, Scope::Predicate | Scope::SelectedRecords)
            || matches!(
                plan.action,
                crate::mutation::Action::DeleteMany | crate::mutation::Action::UpdateMany
            )
        {
            RequiredConfirmation {
                strength: ConfirmationStrength::Target,
                text: target_text,
            }
        } else {
            RequiredConfirmation {
                strength: ConfirmationStrength::Explicit,
                text: "confirm".to_string(),
            }
        }
    }

    fn prepare(
        &self,
        plan: &Plan,
        connection: &ConnectionDefinition,
    ) -> Result<(Effect, MutationPreview, Vec<Warning>, RequiredConfirmation), GuardError> {
        Self::validate(plan, connection)?;

        let execution_digest = self.digest(
            &canonical(&(plan, &policy_snapshot(connection)))
                .map_err(|error| GuardError::InvalidPlan(error.to_string()))?,
            "execution",
        );

        let request = plan.payload.preview();
        let warnings = plan.warnings(connection.environment.kind == EnvironmentKind::Production);
        let required_confirmation = Self::required_confirmation(plan, connection);

        let effect = Effect {
            action: plan.action,
            connection: connection.identity(),
            context: plan.context.clone(),
            target: plan.target.clone(),
            selected_records: plan.selected_records.clone(),
            predicate: plan.predicate.clone(),
            scope: plan.scope,
            impact: plan.impact,
            transaction_behavior: plan.transaction_behavior,
            rollback_availability: plan.rollback_availability,
            execution_mode: plan.execution_mode,
            execution_digest,
            display_digest: String::new(),
        };

        let display_digest = self.digest(
            &canonical(&(&effect, &request, &warnings, &required_confirmation))
                .map_err(|error| GuardError::InvalidPlan(error.to_string()))?,
            "display",
        );

        Ok((
            Effect {
                display_digest,
                ..effect
            },
            request,
            warnings,
            required_confirmation,
        ))
    }

    pub fn preview(
        &mut self,
        plan: &Plan,
        connection: &ConnectionDefinition,
        now: chrono::DateTime<chrono::Utc>,
        lifetime_seconds: i64,
    ) -> Result<Preview, GuardError> {
        if !(MIN_LIFETIME_SECONDS..=MAX_LIFETIME_SECONDS).contains(&lifetime_seconds) {
            return Err(GuardError::InvalidLifetime(lifetime_seconds));
        }
        let _ = self.receipts.purge_expired(now.timestamp_millis());

        let (effect, request, warnings, required_confirmation) = self.prepare(plan, connection)?;
        let identifier = Uuid::new_v4();
        let expires_at = now + chrono::Duration::seconds(lifetime_seconds);

        let payload = TokenPayload {
            version: TOKEN_SCHEMA_VERSION,
            audience: TOKEN_AUDIENCE.to_string(),
            identifier,
            execution_digest: effect.execution_digest.clone(),
            display_digest: effect.display_digest.clone(),
            issued_at_millis: now.timestamp_millis(),
            expires_at_millis: expires_at.timestamp_millis(),
        };
        let encoded = canonical(&payload).map_err(|_| GuardError::MalformedToken)?;
        let token = format!(
            "{}.{}",
            base64url(&encoded),
            base64url(&self.mac(&encoded, "token"))
        );
        if token.len() > MAX_TOKEN_BYTES {
            return Err(GuardError::MalformedToken);
        }

        self.receipts
            .register(Receipt {
                identifier,
                effect_digest: effect.execution_digest.clone(),
                expires_at_millis: expires_at.timestamp_millis(),
            })
            .map_err(|error| GuardError::InvalidPlan(error.to_string()))?;

        Ok(Preview {
            effect,
            request,
            warnings,
            required_confirmation,
            issued_at: now,
            expires_at,
            token,
        })
    }

    fn authenticate(&self, token: &str) -> Result<TokenPayload, GuardError> {
        if token.len() > MAX_TOKEN_BYTES {
            return Err(GuardError::MalformedToken);
        }
        let (payload_part, signature_part) =
            token.split_once('.').ok_or(GuardError::MalformedToken)?;
        if signature_part.contains('.') {
            return Err(GuardError::MalformedToken);
        }
        let encoded = from_base64url(payload_part).ok_or(GuardError::MalformedToken)?;
        let signature = from_base64url(signature_part).ok_or(GuardError::MalformedToken)?;
        if base64url(&encoded) != payload_part || base64url(&signature) != signature_part {
            return Err(GuardError::MalformedToken);
        }

        let mut mac =
            HmacSha256::new_from_slice(&self.signing_key).expect("HMAC takes a key of any size");
        mac.update(&domain_separated(&encoded, "token"));
        mac.verify_slice(&signature)
            .map_err(|_| GuardError::InvalidSignature)?;

        let payload: TokenPayload =
            serde_json::from_slice(&encoded).map_err(|_| GuardError::MalformedToken)?;
        if payload.version != TOKEN_SCHEMA_VERSION || payload.audience != TOKEN_AUDIENCE {
            return Err(GuardError::MalformedToken);
        }
        if canonical(&payload).map_err(|_| GuardError::MalformedToken)? != encoded {
            return Err(GuardError::MalformedToken);
        }
        Ok(payload)
    }

    pub fn authorize(
        &mut self,
        token: &str,
        plan: &Plan,
        connection: &ConnectionDefinition,
        confirmation_text: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Effect, GuardError> {
        let payload = self.authenticate(token)?;
        if payload.expires_at_millis <= now.timestamp_millis() {
            let _ = self.receipts.consume(payload.identifier);
            return Err(GuardError::Expired);
        }

        let (effect, _, _, required_confirmation) = self.prepare(plan, connection)?;
        if effect.execution_digest != payload.execution_digest
            || effect.display_digest != payload.display_digest
        {
            return Err(GuardError::PlanChanged);
        }

        if !constant_time_eq(
            confirmation_text.trim().as_bytes(),
            required_confirmation.text.as_bytes(),
        ) {
            return Err(GuardError::ConfirmationMismatch);
        }

        let receipt = self
            .receipts
            .consume(payload.identifier)
            .map_err(|error| GuardError::InvalidPlan(error.to_string()))?
            .ok_or(GuardError::AlreadyUsed)?;
        if receipt.effect_digest != payload.execution_digest {
            return Err(GuardError::PlanChanged);
        }

        Ok(effect)
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[derive(Serialize)]
struct PolicySnapshot<'a> {
    id: Uuid,
    display_name: &'a str,
    product: crate::product::Product,
    read_only_policy: crate::connection::ReadOnlyPolicy,
    production_policy: crate::connection::ProductionPolicy,
    environment: &'a crate::connection::EnvironmentMetadata,
    location: &'a crate::connection::Location,
    username: Option<&'a str>,
}

fn policy_snapshot(connection: &ConnectionDefinition) -> PolicySnapshot<'_> {
    PolicySnapshot {
        id: connection.id,
        display_name: &connection.display_name,
        product: connection.product_hint,
        read_only_policy: connection.read_only_policy,
        production_policy: connection.production_policy,
        environment: &connection.environment,
        location: &connection.location,
        username: connection.username.as_deref(),
    }
}

fn render_target(target: &TargetIdentifier) -> String {
    match &target.object {
        Some(object) => {
            let path: Vec<String> = object
                .path
                .iter()
                .map(|part| length_prefixed(part))
                .collect();
            format!("{}[{}]", object.kind.key(), path.join("|"))
        }
        None => "connection".to_string(),
    }
}

fn length_prefixed(value: &str) -> String {
    let rendered = printable(value);
    format!("{}:{rendered}", rendered.len())
}

fn printable(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            ':' | '[' | ']' | '|' | '\\' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::*;
    use crate::identify::{ObjectIdentifier, ObjectKind};
    use crate::mutation::*;
    use crate::product::Product;
    use crate::value::Value;

    fn key() -> Vec<u8> {
        (0..=u8::MAX).take(SIGNING_KEY_BYTES).collect()
    }

    fn guard() -> Guard<MemoryReceiptStore> {
        Guard::new(key(), MemoryReceiptStore::default()).unwrap()
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn connection() -> ConnectionDefinition {
        ConnectionDefinition {
            version: SCHEMA_VERSION,
            id: Uuid::from_u128(1),
            display_name: "App".into(),
            product_hint: Product::Postgresql,
            location: Location::Network {
                endpoints: vec![NetworkEndpoint {
                    host: "db".into(),
                    port: Port::new(5432).unwrap(),
                    role: EndpointRole::Primary,
                }],
            },
            username: Some("app".into()),
            namespaces: NamespaceDefaults::default(),
            deployment_mode: DeploymentMode::Automatic,
            authentication: Authentication::default(),
            tls: TlsConfiguration::default(),
            tunnel: None,
            limits: ConnectionLimits::default(),
            read_only_policy: ReadOnlyPolicy::Disabled,
            production_policy: ProductionPolicy::Standard,
            environment: EnvironmentMetadata::default(),
            group: None,
            tags: vec![],
            color: None,
            is_favorite: false,
            created_at: now(),
            updated_at: now(),
            last_tested_at: None,
            last_used_at: None,
        }
    }

    fn plan() -> Plan {
        Plan {
            payload: Payload::Relational {
                product: Product::Postgresql,
                statement: "DELETE FROM \"public\".\"orders\" WHERE id = $1".into(),
                parameters: vec![Parameter {
                    name: "id".into(),
                    value: Value::SignedInteger(7),
                }],
            },
            action: Action::Delete,
            scope: Scope::SingleRecord,
            impact: Impact::exact(1),
            transaction_behavior: TransactionBehavior::Transactional,
            rollback_availability: RollbackAvailability::Available,
            execution_mode: ExecutionMode::Synchronous,
            target: TargetIdentifier::object(
                Uuid::from_u128(1),
                ObjectIdentifier::new(ObjectKind::Table, vec!["public".into(), "orders".into()]),
            ),
            context: MutationContext {
                kind: ContextKind::Database,
                value: "app".into(),
                catalog: None,
                schema: Some("public".into()),
            },
            selected_records: vec![],
            predicate: None,
        }
    }

    #[test]
    fn a_signing_key_of_the_wrong_size_is_refused() {
        assert_eq!(
            Guard::new(vec![0; 16], MemoryReceiptStore::default()).unwrap_err(),
            GuardError::InvalidSigningKey(16)
        );
        assert!(Guard::new(key(), MemoryReceiptStore::default()).is_ok());
    }

    #[test]
    fn a_preview_authorises_the_exact_plan_it_was_taken_for() {
        let mut guard = guard();
        let plan = plan();
        let connection = connection();
        let preview = guard
            .preview(&plan, &connection, now(), DEFAULT_LIFETIME_SECONDS)
            .unwrap();
        let effect = guard
            .authorize(
                &preview.token,
                &plan,
                &connection,
                &preview.required_confirmation.text,
                now(),
            )
            .unwrap();
        assert_eq!(effect.execution_digest, preview.effect.execution_digest);
    }

    #[test]
    fn editing_the_statement_after_the_preview_stops_the_apply() {
        let mut guard = guard();
        let connection = connection();
        let preview = guard
            .preview(&plan(), &connection, now(), DEFAULT_LIFETIME_SECONDS)
            .unwrap();

        let mut tampered = plan();
        tampered.payload = Payload::Relational {
            product: Product::Postgresql,
            statement: "DELETE FROM \"public\".\"orders\"".into(),
            parameters: vec![],
        };
        assert_eq!(
            guard
                .authorize(
                    &preview.token,
                    &tampered,
                    &connection,
                    &preview.required_confirmation.text,
                    now()
                )
                .unwrap_err(),
            GuardError::PlanChanged
        );
    }

    #[test]
    fn editing_any_part_of_the_plan_stops_the_apply() {
        type Tamper = Box<dyn Fn(&mut Plan)>;
        let connection = connection();
        let variants: Vec<(&str, Tamper)> = vec![
            (
                "the parameter",
                Box::new(|plan: &mut Plan| {
                    plan.payload = Payload::Relational {
                        product: Product::Postgresql,
                        statement: "DELETE FROM \"public\".\"orders\" WHERE id = $1".into(),
                        parameters: vec![Parameter {
                            name: "id".into(),
                            value: Value::SignedInteger(8),
                        }],
                    };
                }),
            ),
            (
                "the scope",
                Box::new(|plan: &mut Plan| plan.scope = Scope::EntireObject),
            ),
            (
                "the action",
                Box::new(|plan: &mut Plan| plan.action = Action::Truncate),
            ),
            (
                "the target",
                Box::new(|plan: &mut Plan| {
                    plan.target = TargetIdentifier::object(
                        Uuid::from_u128(1),
                        ObjectIdentifier::new(
                            ObjectKind::Table,
                            vec!["public".into(), "customers".into()],
                        ),
                    );
                }),
            ),
            (
                "the impact",
                Box::new(|plan: &mut Plan| plan.impact = Impact::exact(9_000)),
            ),
        ];

        for (what, mutate) in variants {
            let mut guard = guard();
            let preview = guard
                .preview(&plan(), &connection, now(), DEFAULT_LIFETIME_SECONDS)
                .unwrap();
            let mut tampered = plan();
            mutate(&mut tampered);
            let result = guard.authorize(
                &preview.token,
                &tampered,
                &connection,
                &preview.required_confirmation.text,
                now(),
            );
            assert!(
                matches!(
                    result,
                    Err(GuardError::PlanChanged | GuardError::ConfirmationMismatch)
                ),
                "changing {what} was allowed through: {result:?}"
            );
        }
    }

    #[test]
    fn flipping_the_connection_to_read_only_after_the_preview_stops_the_apply() {
        let mut guard = guard();
        let preview = guard
            .preview(&plan(), &connection(), now(), DEFAULT_LIFETIME_SECONDS)
            .unwrap();
        let mut locked = connection();
        locked.read_only_policy = ReadOnlyPolicy::Required;
        assert_eq!(
            guard
                .authorize(
                    &preview.token,
                    &plan(),
                    &locked,
                    &preview.required_confirmation.text,
                    now()
                )
                .unwrap_err(),
            GuardError::Prohibited(Prohibition::ConnectionReadOnly)
        );
    }

    #[test]
    fn a_token_can_only_be_spent_once() {
        let mut guard = guard();
        let connection = connection();
        let preview = guard
            .preview(&plan(), &connection, now(), DEFAULT_LIFETIME_SECONDS)
            .unwrap();
        let text = preview.required_confirmation.text.clone();
        assert!(guard
            .authorize(&preview.token, &plan(), &connection, &text, now())
            .is_ok());
        assert_eq!(
            guard
                .authorize(&preview.token, &plan(), &connection, &text, now())
                .unwrap_err(),
            GuardError::AlreadyUsed
        );
    }

    #[test]
    fn a_preview_expires() {
        let mut guard = guard();
        let connection = connection();
        let preview = guard.preview(&plan(), &connection, now(), 60).unwrap();
        let later = now() + chrono::Duration::seconds(61);
        assert_eq!(
            guard
                .authorize(
                    &preview.token,
                    &plan(),
                    &connection,
                    &preview.required_confirmation.text,
                    later
                )
                .unwrap_err(),
            GuardError::Expired
        );
    }

    #[test]
    fn a_lifetime_outside_the_range_is_refused() {
        let mut guard = guard();
        for seconds in [0, 4, 901, 86_400] {
            assert_eq!(
                guard
                    .preview(&plan(), &connection(), now(), seconds)
                    .unwrap_err(),
                GuardError::InvalidLifetime(seconds)
            );
        }
    }

    #[test]
    fn a_forged_or_mangled_token_is_refused() {
        let mut guard = guard();
        let connection = connection();
        let preview = guard
            .preview(&plan(), &connection, now(), DEFAULT_LIFETIME_SECONDS)
            .unwrap();
        let text = preview.required_confirmation.text.clone();

        let (payload, signature) = preview.token.split_once('.').unwrap();
        let cases = [
            ("no dot", payload.to_string()),
            ("empty", String::new()),
            ("two dots", format!("{payload}.{signature}.x")),
            (
                "another key's signature",
                format!("{payload}.{}", base64url(&[0u8; 32])),
            ),
            ("garbage payload", format!("!!!!.{signature}")),
        ];
        for (what, token) in cases {
            let result = guard.authorize(&token, &plan(), &connection, &text, now());
            assert!(
                matches!(
                    result,
                    Err(GuardError::MalformedToken | GuardError::InvalidSignature)
                ),
                "{what} was accepted: {result:?}"
            );
        }
    }

    #[test]
    fn a_token_signed_with_a_different_key_does_not_verify() {
        let connection = connection();
        let mut issuer = guard();
        let preview = issuer
            .preview(&plan(), &connection, now(), DEFAULT_LIFETIME_SECONDS)
            .unwrap();

        let mut other =
            Guard::new(vec![9; SIGNING_KEY_BYTES], MemoryReceiptStore::default()).unwrap();
        assert_eq!(
            other
                .authorize(
                    &preview.token,
                    &plan(),
                    &connection,
                    &preview.required_confirmation.text,
                    now()
                )
                .unwrap_err(),
            GuardError::InvalidSignature
        );
    }

    #[test]
    fn the_wrong_confirmation_text_stops_the_apply_and_does_not_burn_the_preview() {
        let mut guard = guard();
        let connection = connection();
        let preview = guard
            .preview(&plan(), &connection, now(), DEFAULT_LIFETIME_SECONDS)
            .unwrap();
        assert_eq!(
            guard
                .authorize(&preview.token, &plan(), &connection, "yes", now())
                .unwrap_err(),
            GuardError::ConfirmationMismatch
        );
        assert!(guard
            .authorize(
                &preview.token,
                &plan(),
                &connection,
                &preview.required_confirmation.text,
                now()
            )
            .is_ok());
    }

    #[test]
    fn a_routine_single_row_delete_asks_only_for_confirm() {
        let confirmation =
            Guard::<MemoryReceiptStore>::required_confirmation(&plan(), &connection());
        assert_eq!(confirmation.strength, ConfirmationStrength::Explicit);
        assert_eq!(confirmation.text, "confirm");
    }

    #[test]
    fn a_predicate_delete_asks_for_the_target() {
        let mut plan = plan();
        plan.scope = Scope::Predicate;
        plan.predicate = Some(crate::paging::Filter::Predicate {
            predicate: crate::paging::FilterPredicate {
                path: vec!["status".into()],
                operator: crate::paging::FilterOperator::Equal,
                values: vec![Value::String("cancelled".into())],
                case_sensitivity: crate::paging::CaseSensitivity::default(),
            },
        });
        let confirmation = Guard::<MemoryReceiptStore>::required_confirmation(&plan, &connection());
        assert_eq!(confirmation.strength, ConfirmationStrength::Target);
        assert_eq!(confirmation.text, "table[6:public|6:orders]");
    }

    #[test]
    fn production_or_a_truncate_asks_for_the_connection_and_the_target() {
        let mut production = connection();
        production.environment.kind = EnvironmentKind::Production;
        let confirmation = Guard::<MemoryReceiptStore>::required_confirmation(&plan(), &production);
        assert_eq!(
            confirmation.strength,
            ConfirmationStrength::ConnectionAndTarget
        );
        assert_eq!(
            confirmation.text,
            "connection[3:App] target[table[6:public|6:orders]]"
        );

        let mut truncate = plan();
        truncate.action = Action::Truncate;
        assert_eq!(
            Guard::<MemoryReceiptStore>::required_confirmation(&truncate, &connection()).strength,
            ConfirmationStrength::ConnectionAndTarget
        );
    }

    #[test]
    fn two_different_targets_cannot_produce_the_same_confirmation_text() {
        let build = |first: &str, second: &str| {
            let mut plan = plan();
            plan.scope = Scope::SelectedRecords;
            plan.selected_records = vec![crate::identify::RecordIdentity {
                kind: crate::identify::RecordIdentityKind::PrimaryKey,
                components: vec![],
                concurrency_tokens: vec![],
            }];
            plan.target = TargetIdentifier::object(
                Uuid::from_u128(1),
                ObjectIdentifier::new(ObjectKind::Table, vec![first.into(), second.into()]),
            );
            Guard::<MemoryReceiptStore>::required_confirmation(&plan, &connection()).text
        };
        assert_ne!(build("a", "bc"), build("ab", "c"));
    }

    #[test]
    fn a_name_with_a_separator_in_it_cannot_forge_a_different_target() {
        let mut plan = plan();
        plan.target = TargetIdentifier::object(
            Uuid::from_u128(1),
            ObjectIdentifier::new(ObjectKind::Table, vec!["ev|il]".into()]),
        );
        let text = Guard::<MemoryReceiptStore>::required_confirmation(&plan, &connection()).text;
        assert!(!text.contains("ev|il]"), "got {text}");
    }

    #[test]
    fn a_predicate_scoped_plan_with_no_predicate_is_refused() {
        let mut guard = guard();
        let mut plan = plan();
        plan.scope = Scope::Predicate;
        plan.predicate = None;
        let error = guard
            .preview(&plan, &connection(), now(), DEFAULT_LIFETIME_SECONDS)
            .unwrap_err();
        assert!(matches!(error, GuardError::InvalidPlan(_)), "got {error:?}");
    }

    #[test]
    fn a_plan_for_the_wrong_product_or_family_is_refused() {
        let mut guard = guard();
        let mut wrong_product = plan();
        wrong_product.payload = Payload::Relational {
            product: Product::Mysql,
            statement: "DELETE FROM x".into(),
            parameters: vec![],
        };
        assert!(matches!(
            guard.preview(
                &wrong_product,
                &connection(),
                now(),
                DEFAULT_LIFETIME_SECONDS
            ),
            Err(GuardError::ProductMismatch { .. })
        ));

        let mut wrong_family = plan();
        wrong_family.payload = Payload::Keyspace {
            product: Product::Postgresql,
            command: "DEL".into(),
            arguments: vec![],
        };
        assert_eq!(
            guard
                .preview(
                    &wrong_family,
                    &connection(),
                    now(),
                    DEFAULT_LIFETIME_SECONDS
                )
                .unwrap_err(),
            GuardError::FamilyMismatch
        );
    }

    #[test]
    fn a_plan_aimed_at_another_connection_is_refused() {
        let mut guard = guard();
        let mut plan = plan();
        plan.target = TargetIdentifier::connection(Uuid::from_u128(99));
        assert!(matches!(
            guard.preview(&plan, &connection(), now(), DEFAULT_LIFETIME_SECONDS),
            Err(GuardError::InvalidPlan(_))
        ));
    }

    #[test]
    fn every_read_only_switch_refuses_the_preview_before_a_token_exists() {
        for (policy, environment, production, expected) in [
            (
                ReadOnlyPolicy::Required,
                EnvironmentProtection::Standard,
                ProductionPolicy::Standard,
                Prohibition::ConnectionReadOnly,
            ),
            (
                ReadOnlyPolicy::Disabled,
                EnvironmentProtection::ReadOnly,
                ProductionPolicy::Standard,
                Prohibition::EnvironmentReadOnly,
            ),
            (
                ReadOnlyPolicy::Disabled,
                EnvironmentProtection::Standard,
                ProductionPolicy::ProhibitMutations,
                Prohibition::ProductionPolicy,
            ),
        ] {
            let mut guard = guard();
            let mut connection = connection();
            connection.read_only_policy = policy;
            connection.environment.protection = environment;
            connection.production_policy = production;
            assert_eq!(
                guard
                    .preview(&plan(), &connection, now(), DEFAULT_LIFETIME_SECONDS)
                    .unwrap_err(),
                GuardError::Prohibited(expected)
            );
        }
    }

    #[test]
    fn an_expired_receipt_is_purged_rather_than_accumulating() {
        let mut store = MemoryReceiptStore::default();
        store
            .register(Receipt {
                identifier: Uuid::from_u128(1),
                effect_digest: "a".into(),
                expires_at_millis: 1_000,
            })
            .unwrap();
        assert_eq!(store.purge_expired(2_000).unwrap(), 1);
        assert!(store.consume(Uuid::from_u128(1)).unwrap().is_none());
    }

    #[test]
    fn the_same_receipt_cannot_be_registered_twice() {
        let mut store = MemoryReceiptStore::default();
        let receipt = Receipt {
            identifier: Uuid::from_u128(1),
            effect_digest: "a".into(),
            expires_at_millis: 1_000,
        };
        assert!(store.register(receipt.clone()).is_ok());
        assert!(store.register(receipt).is_err());
    }

    #[test]
    fn the_two_digests_are_domain_separated() {
        let guard = guard();
        let data = b"the same bytes";
        assert_ne!(
            guard.digest(data, "execution"),
            guard.digest(data, "display")
        );
    }

    #[test]
    fn constant_time_comparison_matches_ordinary_equality() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }
}
