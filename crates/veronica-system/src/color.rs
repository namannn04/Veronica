use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, MessageStream};

pub const SHELL_SCREENSHOT_BUS: &str = "org.gnome.Shell.Screenshot";
pub const SHELL_SCREENSHOT_PATH: &str = "/org/gnome/Shell/Screenshot";
pub const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
pub const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

pub const PICK_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickedColor {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub source: PickSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickSource {
    GnomeShell,
    Portal,
}

impl PickSource {
    pub fn title(self) -> &'static str {
        match self {
            PickSource::GnomeShell => "GNOME Shell",
            PickSource::Portal => "Desktop portal",
        }
    }
}

pub async fn pick() -> Result<PickedColor> {
    let connection = Connection::session()
        .await
        .context("cannot reach the session bus, so no colour can be sampled")?;
    pick_on(&connection).await
}

pub async fn pick_on(connection: &Connection) -> Result<PickedColor> {
    match pick_with_shell(connection).await {
        Ok(color) => return Ok(color),
        Err(error) if is_cancellation(&error) => return Err(error),
        Err(error) => {
            tracing::debug!(
                target: "veronica",
                "GNOME's colour picker is unavailable, trying the portal: {error:#}"
            );
        }
    }
    pick_with_portal(connection).await
}

async fn pick_with_shell(connection: &Connection) -> Result<PickedColor> {
    let reply = connection
        .call_method(
            Some(SHELL_SCREENSHOT_BUS),
            SHELL_SCREENSHOT_PATH,
            Some(SHELL_SCREENSHOT_BUS),
            "PickColor",
            &(),
        )
        .await
        .context("GNOME Shell's colour picker did not answer")?;

    let result: HashMap<String, OwnedValue> = reply
        .body()
        .deserialize()
        .context("GNOME Shell's colour picker returned an unexpected reply")?;
    let (red, green, blue) = triple(&result).context("the sampled colour could not be read")?;
    Ok(PickedColor {
        red,
        green,
        blue,
        source: PickSource::GnomeShell,
    })
}

async fn pick_with_portal(connection: &Connection) -> Result<PickedColor> {
    let token = format!("veronica_{}_{}", std::process::id(), monotonic_tag());

    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")?
        .member("Response")?
        .build();
    zbus::fdo::DBusProxy::new(connection)
        .await?
        .add_match_rule(rule)
        .await
        .context("the bus refused a match rule for the portal's reply")?;
    let mut stream = MessageStream::from(connection.clone());

    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    let reply = connection
        .call_method(
            Some(PORTAL_BUS),
            PORTAL_PATH,
            Some("org.freedesktop.portal.Screenshot"),
            "PickColor",
            &("", options),
        )
        .await
        .context("no colour picker is available on this desktop")?;
    let handle: zbus::zvariant::OwnedObjectPath = reply
        .body()
        .deserialize()
        .context("the portal returned an unexpected request handle")?;

    let deadline = tokio::time::Instant::now() + PICK_TIMEOUT;
    loop {
        let message = tokio::time::timeout_at(deadline, futures_util::StreamExt::next(&mut stream))
            .await
            .map_err(|_| anyhow!("the colour picker was left open, so nothing was sampled"))?
            .ok_or_else(|| anyhow!("the session bus closed before the colour arrived"))?
            .context("the session bus reported an error while waiting for the colour")?;

        let header = message.header();
        if header.path().map(|p| p.as_str()) != Some(handle.as_str()) {
            continue;
        }

        let (response, results): (u32, HashMap<String, OwnedValue>) = message
            .body()
            .deserialize()
            .context("the portal's reply could not be read")?;
        match response {
            0 => {
                let (red, green, blue) =
                    triple(&results).context("the sampled colour could not be read")?;
                return Ok(PickedColor {
                    red,
                    green,
                    blue,
                    source: PickSource::Portal,
                });
            }
            1 => bail!("{CANCELLED}"),
            _ => bail!("the desktop portal refused the colour pick"),
        }
    }
}

pub const CANCELLED: &str = "the colour pick was cancelled";

pub fn is_cancellation(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}").to_lowercase();
    text.contains("cancel") || text.contains("dismissed")
}

fn triple(result: &HashMap<String, OwnedValue>) -> Result<(f64, f64, f64)> {
    let value = result
        .get("color")
        .ok_or_else(|| anyhow!("the reply carried no colour"))?;
    let structure = match unbox(value) {
        Value::Structure(fields) => fields,
        other => bail!(
            "the colour was {} rather than three numbers",
            other.value_signature()
        ),
    };

    let fields = structure.fields();
    if fields.len() != 3 {
        bail!(
            "the colour had {} components rather than three",
            fields.len()
        );
    }
    let mut channels = [0.0f64; 3];
    for (index, field) in fields.iter().enumerate() {
        channels[index] = match unbox(field) {
            Value::F64(v) => *v,
            Value::U8(v) => f64::from(*v) / 255.0,
            other => bail!("colour component {index} was {}", other.value_signature()),
        };
    }
    Ok((channels[0], channels[1], channels[2]))
}

fn unbox<'v>(value: &'v Value<'v>) -> &'v Value<'v> {
    let mut current = value;
    while let Value::Value(inner) = current {
        current = inner;
    }
    current
}

fn monotonic_tag() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Structure;

    fn vardict(value: Value<'static>) -> HashMap<String, OwnedValue> {
        HashMap::from([("color".to_string(), OwnedValue::try_from(value).unwrap())])
    }

    fn ddd(red: f64, green: f64, blue: f64) -> Value<'static> {
        Value::from((red, green, blue))
    }

    fn boxed(fields: [Value<'static>; 3]) -> Value<'static> {
        let [a, b, c] = fields;
        Value::Structure(Structure::from((a, b, c)))
    }

    #[test]
    fn reads_the_three_doubles_the_shell_and_portal_both_send() {
        assert_eq!(
            triple(&vardict(ddd(0.25, 0.5, 0.75))).unwrap(),
            (0.25, 0.5, 0.75)
        );
    }

    #[test]
    fn unwraps_a_colour_nested_inside_a_variant() {
        let doubly = Value::Value(Box::new(ddd(1.0, 0.0, 0.0)));
        assert_eq!(triple(&vardict(doubly)).unwrap(), (1.0, 0.0, 0.0));

        let per_field = boxed([Value::F64(0.1), Value::F64(0.2), Value::F64(0.3)]);
        assert_eq!(triple(&vardict(per_field)).unwrap(), (0.1, 0.2, 0.3));
    }

    #[test]
    fn accepts_byte_components_by_scaling_them() {
        let result = vardict(Value::from((255u8, 0u8, 128u8)));
        let (r, g, b) = triple(&result).unwrap();
        assert_eq!(r, 1.0);
        assert_eq!(g, 0.0);
        assert!((b - 128.0 / 255.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_reply_with_no_colour_is_an_error_not_black() {
        let empty: HashMap<String, OwnedValue> = HashMap::new();
        let error = triple(&empty).unwrap_err().to_string();
        assert!(error.contains("no colour"), "unhelpful message: {error}");
    }

    #[test]
    fn a_colour_of_the_wrong_shape_is_rejected() {
        let error = triple(&vardict(Value::from((0.1f64, 0.2f64))))
            .unwrap_err()
            .to_string();
        assert!(error.contains("components"), "unhelpful message: {error}");

        let text = vardict(Value::Str("#ff0000".into()));
        assert!(triple(&text).is_err(), "a string is not a colour");
    }

    #[test]
    fn cancellation_is_recognised_so_the_fallback_is_not_opened_twice() {
        assert!(is_cancellation(&anyhow!("{CANCELLED}")));
        assert!(is_cancellation(&anyhow!("Operation was cancelled")));
        assert!(!is_cancellation(&anyhow!("no such interface")));
    }

    #[test]
    fn request_tokens_differ_between_picks() {
        assert_ne!(monotonic_tag(), monotonic_tag());
    }
}
