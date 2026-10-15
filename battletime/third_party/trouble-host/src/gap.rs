//! ## Generic Access Profile
//!
//! This profile defines the generic procedures related to discovery of
//! Bluetooth devices (idle mode procedures) and link management aspects
//! of connecting to Bluetooth devices (connecting mode procedures).
//! It also defines procedures related to use of different security levels.
//! In addition, this profile includes common format requirements for
//! parameters accessible on the user interface level.

use crate::prelude::*;

/// Advertising packet is limited to 31 bytes. 9 of these are used by other GAP data, leaving 22 bytes for the Device Name characteristic
const DEVICE_NAME_MAX_LENGTH: usize = 22;

/// The number of attributes added by the GAP and GATT services
/// GAP_SERVICE:                      1
/// ├── DEVICE_NAME:                  2
/// ├── APPEARANCE:                   2
/// └── CENTRAL_ADDRESS_RESOLUTION:   2 (security+central only)
/// GATT_SERVICE:                   + 1
///                                 ---
///                                 = 6 (or 8 with security+central)
#[cfg(not(feature = "security"))]
pub const GAP_SERVICE_ATTRIBUTE_COUNT: usize = 6;
/// The number of attributes added by the GAP and GATT services (with security, peripheral only)
#[cfg(all(feature = "security", not(feature = "central")))]
pub const GAP_SERVICE_ATTRIBUTE_COUNT: usize = 6;
/// The number of attributes added by the GAP and GATT services (with security)
#[cfg(all(feature = "security", feature = "central"))]
pub const GAP_SERVICE_ATTRIBUTE_COUNT: usize = 8;

/// Configuration for the GAP Service.
pub enum GapConfig<'a> {
    /// Peripheral device configuration.
    Peripheral(PeripheralConfig<'a>),
    /// Central device configuration.
    Central(CentralConfig<'a>),
}

/// Configuration for a peripheral device GAP Service.
pub struct PeripheralConfig<'a> {
    /// The name of the peripheral device.
    pub name: &'a str,
    /// The representation of the external appearance of the device.
    ///
    /// Example: `&appearance::sensor::GENERIC_SENSOR.`
    pub appearance: &'a BluetoothUuid16,
    // TODO: Add more GAP parameters
    // pub preferred_connection_parameters: Option<ConnectionParameters>,
}

/// Configuration for a central device GAP Service.
pub struct CentralConfig<'a> {
    /// The name of the central device.
    pub name: &'a str,
    /// The representation of the external appearance of the device.
    ///
    /// Example: `&appearance::sensor::GENERIC_SENSOR`
    pub appearance: &'a BluetoothUuid16,
    // TODO: Add more GAP parameters
}

impl<'a> GapConfig<'a> {
    /// Create a default peripheral configuration.
    ///
    /// This function will create a GAP Service with the following characteristics:
    /// - `name`: The name of the device.
    /// - `appearance`: The appearance of the device, set to `UNKNOWN`.
    pub fn default(name: &'a str) -> Self {
        GapConfig::Peripheral(PeripheralConfig {
            name,
            appearance: &appearance::UNKNOWN,
        })
    }

    /// BattleTime patch: the device name and appearance the GAP service's attributes borrow.
    ///
    /// The attributes themselves come from [`add_gap_attributes`], built in a constant.
    pub fn borrowed_values(self) -> Result<[&'a [u8]; BORROWED_VALUE_SLOTS], &'static str> {
        let (name, appearance) = match self {
            GapConfig::Peripheral(config) => (config.name, config.appearance),
            GapConfig::Central(config) => (config.name, config.appearance),
        };
        if name.len() > DEVICE_NAME_MAX_LENGTH {
            return Err("Device name is too long. Max length is 22 bytes");
        }
        Ok([name.as_bytes(), appearance.as_gatt()])
    }
}

/// The GAP service's slot for the device name.
const DEVICE_NAME_SLOT: u8 = 0;

/// The GAP service's slot for the appearance.
const APPEARANCE_SLOT: u8 = 1;

/// BattleTime patch: adds the GAP and GATT services to a table built in a constant.
///
/// The device name and appearance are borrowed when the server is built, from
/// [`GapConfig::borrowed_values`].
pub const fn add_gap_attributes<const N: usize>(builder: &mut AttributeTableBuilder<N>) {
    builder.add_service(Uuid::Uuid16(service::GAP.to_le_bytes()));
    builder.add_borrowed_characteristic::<str>(
        Uuid::Uuid16(characteristic::DEVICE_NAME.to_le_bytes()),
        DEVICE_NAME_SLOT,
    );
    builder.add_borrowed_characteristic::<BluetoothUuid16>(
        Uuid::Uuid16(characteristic::APPEARANCE.to_le_bytes()),
        APPEARANCE_SLOT,
    );
    #[cfg(all(feature = "security", feature = "central"))]
    builder.add_constant_characteristic::<u8>(
        Uuid::Uuid16(characteristic::CENTRAL_ADDRESS_RESOLUTION.to_le_bytes()),
        PermissionLevel::Allowed,
        || &[1],
    );
    builder.add_service(Uuid::Uuid16(service::GATT.to_le_bytes()));
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn gap_attributes_fill_the_counted_handles() {
        let mut builder = AttributeTableBuilder::<GAP_SERVICE_ATTRIBUTE_COUNT>::new();
        add_gap_attributes(&mut builder);
        assert_eq!(builder.finish().len(), GAP_SERVICE_ATTRIBUTE_COUNT);
    }

    #[test]
    fn a_long_device_name_is_refused() {
        assert!(GapConfig::default("a name far longer than twenty-two bytes")
            .borrowed_values()
            .is_err());
        let values = GapConfig::default("trouble").borrowed_values().unwrap();
        assert_eq!(values[0], b"trouble");
    }
}
