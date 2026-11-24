use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Serialize;
use zbus::names::OwnedInterfaceName;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Connection;

pub const BLUEZ_BUS: &str = "org.bluez";
pub const ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
pub const DEVICE_INTERFACE: &str = "org.bluez.Device1";
pub const BATTERY_INTERFACE: &str = "org.bluez.Battery1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adapter {
    pub id: String,
    pub name: String,
    pub address: String,
    pub powered: bool,
    pub discovering: bool,
    pub discoverable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub address: String,
    pub name: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub battery_percent: Option<u8>,
    pub rssi: Option<i16>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothState {
    pub adapters: Vec<Adapter>,
    pub devices: Vec<Device>,
    pub unavailable: Option<String>,
}

impl BluetoothState {
    pub fn connected(&self) -> Vec<&Device> {
        self.devices
            .iter()
            .filter(|device| device.connected)
            .collect()
    }

    pub fn powered(&self) -> bool {
        self.adapters.iter().any(|adapter| adapter.powered)
    }

    pub fn summary(&self) -> String {
        if let Some(reason) = &self.unavailable {
            return reason.clone();
        }
        if self.adapters.is_empty() {
            return "No Bluetooth adapter".to_string();
        }
        if !self.powered() {
            return "Bluetooth is off".to_string();
        }
        let connected = self.connected();
        match connected.len() {
            0 => "Bluetooth on, nothing connected".to_string(),
            1 => format!("Connected to {}", connected[0].name),
            count => format!("{count} devices connected"),
        }
    }
}

fn string_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    properties
        .get(key)
        .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
}

fn bool_property(properties: &HashMap<String, OwnedValue>, key: &str) -> bool {
    properties
        .get(key)
        .and_then(|value| bool::try_from(value.try_clone().ok()?).ok())
        .unwrap_or(false)
}

fn object_name(path: &OwnedObjectPath) -> String {
    path.as_str()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>>;

fn interface(name: &str) -> OwnedInterfaceName {
    OwnedInterfaceName::try_from(name).expect("the interface names above are valid")
}

fn shape(objects: &ManagedObjects) -> (Vec<Adapter>, Vec<Device>) {
    let mut adapters = Vec::new();
    let mut devices = Vec::new();

    for (path, interfaces) in objects {
        if let Some(properties) = interfaces.get(&interface(ADAPTER_INTERFACE)) {
            let id = object_name(path);
            adapters.push(Adapter {
                name: string_property(properties, "Alias")
                    .or_else(|| string_property(properties, "Name"))
                    .unwrap_or_else(|| id.clone()),
                id,
                address: string_property(properties, "Address").unwrap_or_default(),
                powered: bool_property(properties, "Powered"),
                discovering: bool_property(properties, "Discovering"),
                discoverable: bool_property(properties, "Discoverable"),
            });
        }

        if let Some(properties) = interfaces.get(&interface(DEVICE_INTERFACE)) {
            let address = string_property(properties, "Address").unwrap_or_default();
            devices.push(Device {
                name: string_property(properties, "Alias")
                    .or_else(|| string_property(properties, "Name"))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| address.clone()),
                address,
                paired: bool_property(properties, "Paired"),
                trusted: bool_property(properties, "Trusted"),
                connected: bool_property(properties, "Connected"),
                battery_percent: interfaces
                    .get(&interface(BATTERY_INTERFACE))
                    .and_then(|battery| battery.get("Percentage"))
                    .and_then(|value| value.try_clone().ok())
                    .and_then(|value| u8::try_from(value).ok()),
                rssi: properties
                    .get("RSSI")
                    .and_then(|value| value.try_clone().ok())
                    .and_then(|value| i16::try_from(value).ok()),
                icon: string_property(properties, "Icon"),
            });
        }
    }

    adapters.sort_by(|left, right| left.id.cmp(&right.id));
    devices.sort_by(|left, right| {
        right
            .connected
            .cmp(&left.connected)
            .then(right.paired.cmp(&left.paired))
            .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    (adapters, devices)
}

async fn managed_objects(connection: &Connection) -> Result<ManagedObjects> {
    let proxy = zbus::fdo::ObjectManagerProxy::builder(connection)
        .destination(BLUEZ_BUS)?
        .path("/")?
        .build()
        .await
        .context("BlueZ does not export an object manager")?;
    let objects = proxy
        .get_managed_objects()
        .await
        .context("BlueZ did not answer GetManagedObjects")?;
    Ok(objects)
}

pub async fn state() -> BluetoothState {
    let connection = match Connection::system().await {
        Ok(connection) => connection,
        Err(error) => {
            return BluetoothState {
                unavailable: Some(format!("The system bus is not reachable: {error}")),
                ..Default::default()
            }
        }
    };
    read(&connection).await
}

pub async fn read(connection: &Connection) -> BluetoothState {
    let objects = match managed_objects(connection).await {
        Ok(objects) => objects,
        Err(error) => {
            tracing::debug!("bluetooth unavailable: {error:#}");
            return BluetoothState {
                unavailable: Some(
                    "BlueZ is not running; install and start bluetooth.service to see devices."
                        .to_string(),
                ),
                ..Default::default()
            };
        }
    };

    let (adapters, devices) = shape(&objects);
    if adapters.is_empty() {
        return BluetoothState {
            adapters,
            devices,
            unavailable: Some("This computer has no Bluetooth adapter.".to_string()),
        };
    }
    BluetoothState {
        adapters,
        devices,
        unavailable: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(value: impl Into<zbus::zvariant::Value<'static>>) -> OwnedValue {
        OwnedValue::try_from(value.into()).unwrap()
    }

    fn properties(pairs: Vec<(&str, OwnedValue)>) -> HashMap<String, OwnedValue> {
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect()
    }

    fn objects() -> ManagedObjects {
        let mut objects: ManagedObjects = HashMap::new();
        objects.insert(
            OwnedObjectPath::try_from("/org/bluez/hci0").unwrap(),
            HashMap::from([(
                interface(ADAPTER_INTERFACE),
                properties(vec![
                    ("Address", owned("10:68:38:C3:E3:C0")),
                    ("Alias", owned("This computer")),
                    ("Powered", owned(true)),
                    ("Discovering", owned(false)),
                    ("Discoverable", owned(false)),
                ]),
            )]),
        );
        objects.insert(
            OwnedObjectPath::try_from("/org/bluez/hci0/dev_AA_BB").unwrap(),
            HashMap::from([
                (
                    interface(DEVICE_INTERFACE),
                    properties(vec![
                        ("Address", owned("AA:BB:CC:DD:EE:FF")),
                        ("Alias", owned("WH-1000XM4")),
                        ("Paired", owned(true)),
                        ("Trusted", owned(true)),
                        ("Connected", owned(true)),
                        ("Icon", owned("audio-headset")),
                    ]),
                ),
                (
                    interface(BATTERY_INTERFACE),
                    properties(vec![("Percentage", owned(70u8))]),
                ),
            ]),
        );
        objects.insert(
            OwnedObjectPath::try_from("/org/bluez/hci0/dev_11_22").unwrap(),
            HashMap::from([(
                interface(DEVICE_INTERFACE),
                properties(vec![
                    ("Address", owned("11:22:33:44:55:66")),
                    ("Paired", owned(false)),
                    ("Connected", owned(false)),
                    ("RSSI", owned(-64i16)),
                ]),
            )]),
        );
        objects
    }

    #[test]
    fn adapters_and_devices_are_read_off_their_own_interfaces() {
        let (adapters, devices) = shape(&objects());
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].id, "hci0");
        assert_eq!(adapters[0].name, "This computer");
        assert!(adapters[0].powered);
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn a_battery_on_the_sibling_interface_lands_on_the_right_device() {
        let (_, devices) = shape(&objects());
        let headphones = devices.iter().find(|d| d.name == "WH-1000XM4").unwrap();
        assert_eq!(headphones.battery_percent, Some(70));
    }

    #[test]
    fn a_device_that_reports_no_battery_is_none_rather_than_zero() {
        let (_, devices) = shape(&objects());
        let other = devices
            .iter()
            .find(|d| d.address == "11:22:33:44:55:66")
            .unwrap();
        assert_eq!(other.battery_percent, None, "showing 0% would be a lie");
        assert_eq!(other.rssi, Some(-64));
    }

    #[test]
    fn an_unnamed_device_falls_back_to_its_address_rather_than_an_empty_row() {
        let (_, devices) = shape(&objects());
        let other = devices
            .iter()
            .find(|d| d.address == "11:22:33:44:55:66")
            .unwrap();
        assert_eq!(other.name, "11:22:33:44:55:66");
    }

    #[test]
    fn connected_devices_sort_ahead_of_the_rest() {
        let (_, devices) = shape(&objects());
        assert!(devices[0].connected, "got {devices:#?}");
    }

    #[test]
    fn the_summary_names_a_single_connected_device() {
        let (adapters, devices) = shape(&objects());
        let state = BluetoothState {
            adapters,
            devices,
            unavailable: None,
        };
        assert_eq!(state.summary(), "Connected to WH-1000XM4");
        assert_eq!(state.connected().len(), 1);
    }

    #[test]
    fn a_powered_off_adapter_says_so_instead_of_reporting_no_devices() {
        let (mut adapters, devices) = shape(&objects());
        adapters[0].powered = false;
        let state = BluetoothState {
            adapters,
            devices,
            unavailable: None,
        };
        assert!(!state.powered());
        assert_eq!(state.summary(), "Bluetooth is off");
    }

    #[test]
    fn an_unreachable_daemon_reports_why_rather_than_an_empty_list() {
        let state = BluetoothState {
            unavailable: Some("BlueZ is not running".to_string()),
            ..Default::default()
        };
        assert_eq!(state.summary(), "BlueZ is not running");
    }

    #[test]
    fn a_machine_with_no_adapter_is_distinguished_from_one_with_a_radio_off() {
        let state = BluetoothState::default();
        assert_eq!(state.summary(), "No Bluetooth adapter");
    }
}
