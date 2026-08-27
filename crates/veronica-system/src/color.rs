//! Sampling a colour from the screen.
//!
//! Edith calls `NSColorSampler`. There is no single Linux equivalent, so
//! Veronica tries the two that exist, in order:
//!
//! 1. `org.gnome.Shell.Screenshot.PickColor`, GNOME's own eyedropper. It is a
//!    plain round trip that returns the colour, and it is what the shell's
//!    screenshot UI itself uses.
//! 2. `org.freedesktop.portal.Screenshot.PickColor`, the portal interface. It
//!    is asynchronous — the call returns a request handle and the colour
//!    arrives on a signal — and it works on any desktop with a portal, which is
//!    why it is the fallback rather than GNOME-only code.
//!
//! Both hand back RGB in 0..=1, matching what the swatch history stores.
//!
//! Neither can be sampled without the user clicking, so there is no way to read
//! the screen behind their back: cancelling the eyedropper is reported as a
//! cancellation, not a colour.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, MessageStream};

pub const SHELL_SCREENSHOT_BUS: &str = "org.gnome.Shell.Screenshot";
pub const SHELL_SCREENSHOT_PATH: &str = "/org/gnome/Shell/Screenshot";
pub const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
pub const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

/// Longest a pick may stay open. The eyedropper waits for a click, so this only
/// bounds the case where the dialog is abandoned and never answered — without
/// it the caller would wait for the lifetime of the process.
pub const PICK_TIMEOUT: Duration = Duration::from_secs(300);

/// An sRGB triple in 0..=1, exactly as the compositor reported it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickedColor {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    /// Which of the two backends answered, so the interface can say so.
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

/// Pick a colour, preferring the shell and falling back to the portal.
///
/// A cancellation from the first backend is returned as-is rather than retried
/// through the second: the user closed the eyedropper, and opening a second one
/// would be a surprise.
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

/// GNOME Shell's eyedropper. Returns `a{sv}` with `color` as `(ddd)`.
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

/// The portal's eyedropper: call, then wait for the request's `Response` signal.
async fn pick_with_portal(connection: &Connection) -> Result<PickedColor> {
    // A unique token keeps this request's object path distinct from any other
    // portal request this process makes.
    let token = format!("veronica_{}_{}", std::process::id(), monotonic_tag());

    // Subscribe before calling. The portal may answer immediately, and a
    // subscription set up afterwards would miss that signal.
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
            // No parent window: Veronica's picker is reachable from the tray and
            // the shell as well as the app, so there is not always a window to
            // parent to.
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
            // Another portal request's reply; not ours.
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

/// Wording used for a user-cancelled pick, so callers can recognise it without
/// a dedicated error type crossing the IPC boundary as a string.
pub const CANCELLED: &str = "the colour pick was cancelled";

pub fn is_cancellation(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}").to_lowercase();
    text.contains("cancel") || text.contains("dismissed")
}

/// Pull `color` out of a portal or shell result vardict.
///
/// The value is documented as `(ddd)`, but how deeply it is boxed varies: the
/// vardict itself makes it a variant, and an implementation that builds the
/// struct from variants leaves each component boxed too. Both levels are
/// unwrapped so any of those shapes reads the same.
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
        bail!("the colour had {} components rather than three", fields.len());
    }
    let mut channels = [0.0f64; 3];
    for (index, field) in fields.iter().enumerate() {
        channels[index] = match unbox(field) {
            Value::F64(v) => *v,
            // Tolerated because a reimplemented portal could plausibly send
            // integers; clamping happens in the caller either way.
            Value::U8(v) => f64::from(*v) / 255.0,
            other => bail!("colour component {index} was {}", other.value_signature()),
        };
    }
    Ok((channels[0], channels[1], channels[2]))
}

/// Strip any number of `Value::Value` wrappers off a value.
fn unbox<'v>(value: &'v Value<'v>) -> &'v Value<'v> {
    let mut current = value;
    while let Value::Value(inner) = current {
        current = inner;
    }
    current
}

/// A cheap changing suffix for the request token. Not security-relevant; it only
/// has to differ between two picks from the same process.
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

    /// A genuine `(ddd)` struct, the signature both services document.
    fn ddd(red: f64, green: f64, blue: f64) -> Value<'static> {
        Value::from((red, green, blue))
    }

    /// The same struct built out of variants, which is what an implementation
    /// assembling it from a vardict produces: signature `(vvv)`.
    fn boxed(fields: [Value<'static>; 3]) -> Value<'static> {
        let [a, b, c] = fields;
        Value::Structure(Structure::from((a, b, c)))
    }

    #[test]
    fn reads_the_three_doubles_the_shell_and_portal_both_send() {
        assert_eq!(triple(&vardict(ddd(0.25, 0.5, 0.75))).unwrap(), (0.25, 0.5, 0.75));
    }

    #[test]
    fn unwraps_a_colour_nested_inside_a_variant() {
        // The portal builds its results as a{sv}, and some implementations box
        // the struct one level deeper than others.
        let doubly = Value::Value(Box::new(ddd(1.0, 0.0, 0.0)));
        assert_eq!(triple(&vardict(doubly)).unwrap(), (1.0, 0.0, 0.0));

        // And with each component boxed rather than the struct.
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
