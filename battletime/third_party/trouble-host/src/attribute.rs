//! Attribute protocol implementation.
use core::cell::RefCell;
use core::fmt;
use core::marker::PhantomData;

use bt_hci::uuid::declarations::{CHARACTERISTIC, PRIMARY_SERVICE, SECONDARY_SERVICE};
use bt_hci::uuid::descriptors::CLIENT_CHARACTERISTIC_CONFIGURATION;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::with_timeout;

use crate::att::{AttErrorCode, AttUns};
use crate::attribute_server::AttributeServer;
use crate::cursor::ReadCursor;
use crate::gatt::ATT_TRANSACTION_TIMEOUT;
use crate::prelude::{AsGatt, FixedGattValue, FromGatt, GattConnection, SecurityLevel};
use crate::types::gatt_traits::FromGattError;
pub use crate::types::uuid::Uuid;
use crate::{gatt, Error, PacketPool, MAX_INVALID_DATA_LEN};

/// The maximum size in bytes of an attribute using inline small data storage.
pub const MAX_SMALL_DATA_SIZE: usize = 20;

/// Characteristic properties
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum CharacteristicProp {
    /// Broadcast
    Broadcast = 0x01,
    /// Read
    Read = 0x02,
    /// Write without response
    WriteWithoutResponse = 0x04,
    /// Write
    Write = 0x08,
    /// Notify
    Notify = 0x10,
    /// Indicate
    Indicate = 0x20,
    /// Authenticated writes
    AuthenticatedWrite = 0x40,
    /// Extended properties
    Extended = 0x80,
}

/// Attribute permissions
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PermissionLevel {
    #[default]
    /// Operation is allowed with no encryption or authentication
    Allowed,
    /// Encryption is required
    EncryptionRequired,
    /// Encryption and authentication are required
    AuthenticationRequired,
    /// Operation is not allowed
    NotAllowed,
}

/// Attribute permissions
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AttPermissions {
    /// Security required for read operations
    pub read: PermissionLevel,
    /// Security required for write operations
    pub write: PermissionLevel,
    /// Minimum encryption key length required (0 = no minimum)
    #[cfg(feature = "legacy-pairing")]
    pub min_key_len: u8,
}

impl AttPermissions {
    /// BattleTime patch: permissions with no minimum key length, for tables built in a constant.
    pub const fn new(read: PermissionLevel, write: PermissionLevel) -> Self {
        Self {
            read,
            write,
            #[cfg(feature = "legacy-pairing")]
            min_key_len: 0,
        }
    }

    pub(crate) const fn read_only() -> Self {
        Self::new(PermissionLevel::Allowed, PermissionLevel::NotAllowed)
    }

    pub(crate) fn can_read(
        &self,
        level: SecurityLevel,
        #[cfg(feature = "legacy-pairing")] encryption_key_len: u8,
    ) -> Result<(), AttErrorCode> {
        match self.read {
            PermissionLevel::NotAllowed => Err(AttErrorCode::READ_NOT_PERMITTED),
            PermissionLevel::EncryptionRequired | PermissionLevel::AuthenticationRequired
                if level < SecurityLevel::Encrypted =>
            {
                Err(AttErrorCode::INSUFFICIENT_AUTHENTICATION)
            }
            PermissionLevel::AuthenticationRequired if level < SecurityLevel::EncryptedAuthenticated => {
                Err(AttErrorCode::INSUFFICIENT_AUTHENTICATION)
            }
            _ => {
                #[cfg(feature = "legacy-pairing")]
                if self.min_key_len > 0 && level.encrypted() && encryption_key_len < self.min_key_len {
                    return Err(AttErrorCode::INSUFFICIENT_ENCRYPTION_KEY_SIZE);
                }
                Ok(())
            }
        }
    }

    pub(crate) fn can_write(
        &self,
        level: SecurityLevel,
        #[cfg(feature = "legacy-pairing")] encryption_key_len: u8,
    ) -> Result<(), AttErrorCode> {
        match self.write {
            PermissionLevel::NotAllowed => Err(AttErrorCode::WRITE_NOT_PERMITTED),
            PermissionLevel::EncryptionRequired | PermissionLevel::AuthenticationRequired
                if level < SecurityLevel::Encrypted =>
            {
                Err(AttErrorCode::INSUFFICIENT_AUTHENTICATION)
            }
            PermissionLevel::AuthenticationRequired if level < SecurityLevel::EncryptedAuthenticated => {
                Err(AttErrorCode::INSUFFICIENT_AUTHENTICATION)
            }
            _ => {
                #[cfg(feature = "legacy-pairing")]
                if self.min_key_len > 0 && level.encrypted() && encryption_key_len < self.min_key_len {
                    return Err(AttErrorCode::INSUFFICIENT_ENCRYPTION_KEY_SIZE);
                }
                Ok(())
            }
        }
    }
}

/// A GATT characteristic declaration value, as a GATT client reads it.
pub(crate) struct CharacteristicDeclaration {
    pub props: CharacteristicProps,
    pub value_handle: u16,
    pub uuid: Uuid,
}

impl TryFrom<&[u8]> for CharacteristicDeclaration {
    type Error = Error;

    fn try_from(data: &'_ [u8]) -> Result<Self, Self::Error> {
        let mut r = ReadCursor::new(data);
        Ok(CharacteristicDeclaration {
            props: CharacteristicProps(r.read()?),
            value_handle: r.read()?,
            uuid: Uuid::try_from(r.remaining())?,
        })
    }
}

// BattleTime patch: a server's attribute table is built in a constant when the firmware is compiled,
// so the declarations, UUIDs and constant values stay in flash, and only the values that change
// at run time are kept in RAM. See BATTLETIME.md.

/// One attribute of a table built in a constant, so it is placed in flash.
#[derive(Clone, Copy, Debug)]
pub struct Attribute {
    /// The attribute's type, except for a service declaration, where it is the service's UUID.
    stored_uuid: Uuid,
    pub(crate) permissions: AttPermissions,
    pub(crate) data: AttributeData,
}

/// Where an attribute's value comes from.
#[derive(Clone, Copy, Debug)]
pub(crate) enum AttributeData {
    /// A service declaration, whose value is the service's UUID, kept as the attribute's UUID.
    Service { secondary: bool },
    /// A characteristic declaration; its value attribute is always the next handle and holds its UUID.
    Declaration { props: CharacteristicProps },
    /// A value fixed when the firmware is compiled.
    Constant { value: fn() -> &'static [u8] },
    /// A value lent to the table when the server is built, such as the device name.
    Borrowed { slot: u8 },
    /// A value kept in the table's RAM, starting from what `initial` writes or from zero bytes.
    Stored {
        offset: u16,
        capacity: u16,
        variable_len: bool,
        initial: Option<InitialValue>,
    },
    /// A value kept for each client, such as a CCCD.
    ClientSpecific { variable_len: bool, capacity: u16 },
}

/// Writes a stored value's starting bytes into the buffer it is given and returns how many it wrote.
pub type InitialValue = fn(&mut [u8]) -> usize;

/// Writes `value` as a stored value's starting bytes, for an [`InitialValue`] the GATT macros generate.
pub fn write_initial_value<T: AsGatt + ?Sized>(buffer: &mut [u8], value: &T) -> usize {
    let bytes = value.as_gatt();
    assert!(
        bytes.len() <= buffer.len(),
        "an initial value is longer than its attribute"
    );
    buffer[..bytes.len()].copy_from_slice(bytes);
    bytes.len()
}

/// The number of values a table can borrow when its server is built: the GAP service's two.
pub const BORROWED_VALUE_SLOTS: usize = 2;

/// The bytes before a variable-length stored value that hold its length.
const STORED_LENGTH_BYTES: usize = 2;

/// The longest characteristic declaration value: properties, value handle and a 128-bit UUID.
const LONGEST_DECLARATION: usize = 1 + 2 + 16;

impl Attribute {
    const UNUSED: Attribute = Attribute {
        stored_uuid: Uuid::Uuid16([0, 0]),
        permissions: AttPermissions::new(PermissionLevel::NotAllowed, PermissionLevel::NotAllowed),
        data: AttributeData::ClientSpecific {
            variable_len: false,
            capacity: 0,
        },
    };

    /// The attribute's type.
    pub(crate) fn uuid(&self) -> Uuid {
        match self.data {
            AttributeData::Service { secondary: false } => Uuid::Uuid16(PRIMARY_SERVICE.to_le_bytes()),
            AttributeData::Service { secondary: true } => Uuid::Uuid16(SECONDARY_SERVICE.to_le_bytes()),
            _ => self.stored_uuid,
        }
    }

    pub(crate) fn permissions(&self) -> AttPermissions {
        self.permissions
    }

    pub(crate) fn readable(&self) -> bool {
        self.permissions.read != PermissionLevel::NotAllowed
    }

    pub(crate) fn writable(&self) -> bool {
        self.permissions.write != PermissionLevel::NotAllowed
    }

    pub(crate) fn is_client_specific(&self) -> bool {
        matches!(self.data, AttributeData::ClientSpecific { .. })
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Attribute {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", defmt::Debug2Format(self))
    }
}

/// The part of a table kept in RAM: the stored values and the borrowed ones.
struct AttributeValues<'d> {
    stored: &'d mut [u8],
    borrowed: [&'d [u8]; BORROWED_VALUE_SLOTS],
}

/// Finds a stored value's bytes in the table's RAM.
fn stored_bytes(stored: &[u8], offset: u16, capacity: u16, variable_len: bool) -> &[u8] {
    let offset = usize::from(offset);
    if variable_len {
        let length = usize::from(u16::from_le_bytes([stored[offset], stored[offset + 1]]));
        &stored[offset + STORED_LENGTH_BYTES..][..length]
    } else {
        &stored[offset..][..usize::from(capacity)]
    }
}

/// Writes into a stored value as an ATT write does, setting a variable-length value's length.
fn write_stored(
    stored: &mut [u8],
    offset: u16,
    capacity: u16,
    variable_len: bool,
    write_offset: usize,
    data: &[u8],
) -> Result<(), AttErrorCode> {
    let offset = usize::from(offset);
    let length = if variable_len {
        usize::from(u16::from_le_bytes([stored[offset], stored[offset + 1]]))
    } else {
        usize::from(capacity)
    };
    if write_offset > length {
        return Err(AttErrorCode::INVALID_OFFSET);
    }
    if write_offset + data.len() > usize::from(capacity) {
        return Err(AttErrorCode::INVALID_ATTRIBUTE_VALUE_LENGTH);
    }
    let value_start = if variable_len {
        offset + STORED_LENGTH_BYTES
    } else {
        offset
    };
    stored[value_start + write_offset..][..data.len()].copy_from_slice(data);
    if variable_len {
        let new_length = (write_offset + data.len()) as u16;
        stored[offset..offset + STORED_LENGTH_BYTES].copy_from_slice(&new_length.to_le_bytes());
    }
    Ok(())
}

/// Copies as much of `source`, from `offset` on, as fits into `destination`, as an ATT read does.
fn read_from(source: &[u8], offset: usize, destination: &mut [u8]) -> Result<usize, AttErrorCode> {
    if offset > source.len() {
        return Err(AttErrorCode::INVALID_OFFSET);
    }
    let length = destination.len().min(source.len() - offset);
    destination[..length].copy_from_slice(&source[offset..][..length]);
    Ok(length)
}

/// A table of attributes: the attributes themselves in flash, and the values that change in RAM.
pub struct AttributeTable<'d, M: RawMutex, const MAX: usize> {
    attributes: &'static [Attribute],
    values: Mutex<M, RefCell<AttributeValues<'d>>>,
}

impl<'d, M: RawMutex, const MAX: usize> AttributeTable<'d, M, MAX> {
    /// BattleTime patch: a table over attributes built by an [`AttributeTableBuilder`], with RAM for its stored values.
    ///
    /// `stored` must hold at least the builder's `stored_bytes`; each stored value starts from what
    /// its initial value writes, or from zero bytes when it has none.
    pub fn new(
        attributes: &'static [Attribute],
        stored: &'d mut [u8],
        borrowed: [&'d [u8]; BORROWED_VALUE_SLOTS],
    ) -> Self {
        assert!(
            attributes.len() <= MAX,
            "the attribute table is larger than its server allows"
        );
        stored.fill(0);
        for attribute in attributes {
            if let AttributeData::Stored {
                offset,
                capacity,
                variable_len,
                initial: Some(initial),
            } = attribute.data
            {
                let offset = usize::from(offset);
                let value_start = if variable_len {
                    offset + STORED_LENGTH_BYTES
                } else {
                    offset
                };
                let length = initial(&mut stored[value_start..][..usize::from(capacity)]) as u16;
                if variable_len {
                    stored[offset..value_start].copy_from_slice(&length.to_le_bytes());
                }
            }
        }
        Self {
            attributes,
            values: Mutex::new(RefCell::new(AttributeValues { stored, borrowed })),
        }
    }

    pub(crate) fn iterate<F: FnOnce(AttributeIterator<'static>) -> R, R>(&self, f: F) -> R {
        f(AttributeIterator {
            attributes: self.attributes,
            pos: 0,
        })
    }

    pub(crate) fn iterate_from<F: FnOnce(AttributeIterator<'static>) -> R, R>(&self, start: u16, f: F) -> R {
        f(AttributeIterator {
            attributes: self.attributes,
            pos: usize::from(start).saturating_sub(1),
        })
    }

    pub(crate) fn with_attribute<F: FnOnce(&'static Attribute) -> R, R>(&self, handle: u16, f: F) -> Option<R> {
        if handle == 0 {
            return None;
        }
        self.attributes.get(usize::from(handle) - 1).map(f)
    }

    /// Passes the attribute's value to `f`, or `None` for a value kept for each client.
    pub(crate) fn with_value<F: FnOnce(Option<&[u8]>) -> R, R>(&self, handle: u16, attribute: &Attribute, f: F) -> R {
        match attribute.data {
            AttributeData::Service { .. } => f(Some(attribute.stored_uuid.as_raw())),
            AttributeData::Declaration { props } => {
                let mut bytes = [0u8; LONGEST_DECLARATION];
                let uuid = self.attributes.get(usize::from(handle)).map(Attribute::uuid);
                let uuid_bytes = uuid.as_ref().map_or(&[][..], Uuid::as_raw);
                bytes[0] = props.to_raw();
                bytes[1..3].copy_from_slice(&(handle + 1).to_le_bytes());
                bytes[3..3 + uuid_bytes.len()].copy_from_slice(uuid_bytes);
                f(Some(&bytes[..3 + uuid_bytes.len()]))
            }
            AttributeData::Constant { value } => f(Some(value())),
            AttributeData::Borrowed { slot } => {
                let value = self.values.lock(|values| values.borrow().borrowed[usize::from(slot)]);
                f(Some(value))
            }
            AttributeData::Stored {
                offset,
                capacity,
                variable_len,
                ..
            } => self.values.lock(|values| {
                let values = values.borrow();
                f(Some(stored_bytes(values.stored, offset, capacity, variable_len)))
            }),
            AttributeData::ClientSpecific { .. } => f(None),
        }
    }

    /// Reads the attribute's value from `offset`, as an ATT read does.
    pub(crate) fn read_value(
        &self,
        handle: u16,
        attribute: &Attribute,
        offset: usize,
        data: &mut [u8],
    ) -> Result<usize, AttErrorCode> {
        self.with_value(handle, attribute, |value| match value {
            Some(value) => read_from(value, offset, data),
            None => Err(AttErrorCode::UNLIKELY_ERROR),
        })
    }

    /// Writes the attribute's value from `offset`, as an ATT write does; only stored values take writes.
    pub(crate) fn write_value(&self, attribute: &Attribute, offset: usize, data: &[u8]) -> Result<(), AttErrorCode> {
        match attribute.data {
            AttributeData::Stored {
                offset: stored_offset,
                capacity,
                variable_len,
                ..
            } => self.values.lock(|values| {
                let mut values = values.borrow_mut();
                write_stored(values.stored, stored_offset, capacity, variable_len, offset, data)
            }),
            _ => Err(AttErrorCode::WRITE_NOT_PERMITTED),
        }
    }

    /// The most bytes the attribute's value can hold.
    pub(crate) fn capacity(&self, handle: u16, attribute: &Attribute) -> usize {
        match attribute.data {
            AttributeData::Stored { capacity, .. } | AttributeData::ClientSpecific { capacity, .. } => {
                usize::from(capacity)
            }
            _ => self.with_value(handle, attribute, |value| value.map_or(0, <[u8]>::len)),
        }
    }

    pub(crate) fn is_variable_len(attribute: &Attribute) -> bool {
        match attribute.data {
            AttributeData::Stored { variable_len, .. } | AttributeData::ClientSpecific { variable_len, .. } => {
                variable_len
            }
            _ => false,
        }
    }

    /// Get the permissions for the attribute
    ///
    /// Returns `None` if the attribute handle is invalid.
    pub fn permissions(&self, attribute: u16) -> Option<AttPermissions> {
        self.with_attribute(attribute, |att| att.permissions())
    }

    /// Get the UUID of the attribute type
    ///
    /// Returns `None` if the attribute handle is invalid.
    pub fn uuid(&self, attribute: u16) -> Option<Uuid> {
        self.with_attribute(attribute, |att| att.uuid())
    }

    /// Lends a borrowed value, such as the device name, a new value.
    pub(crate) fn set_ro(&self, attribute: u16, new_value: &'d [u8]) -> Result<(), Error> {
        self.with_attribute(attribute, |att| match att.data {
            AttributeData::Borrowed { slot } => {
                self.values
                    .lock(|values| values.borrow_mut().borrowed[usize::from(slot)] = new_value);
                Ok(())
            }
            _ => Err(Error::NotSupported),
        })
        .unwrap_or(Err(Error::NotFound))
    }

    /// Read the raw value of the attribute
    ///
    /// If the attribute value is larger than the data buffer, data will be filled with
    /// as many bytes as fit. Use additional reads with an offset to read the remaining data.
    ///
    /// The value of the attribute is undefined for connection-specific attributes (like CCCD).
    pub fn read(&self, attribute: u16, offset: usize, data: &mut [u8]) -> Result<usize, Error> {
        self.with_attribute(attribute, |att| {
            self.read_value(attribute, att, offset, data).map_err(Into::into)
        })
        .unwrap_or(Err(Error::NotFound))
    }

    /// Write the raw value of the attribute
    ///
    /// If the attribute is variable length, its length will be set to `offset + data.len()`.
    /// If the attribute is fixed length, the range `offset..(offset + data.len())` will be
    /// overwritten.
    pub fn write(&self, attribute: u16, offset: usize, data: &[u8]) -> Result<(), Error> {
        self.with_attribute(attribute, |att| self.write_value(att, offset, data).map_err(Into::into))
            .unwrap_or(Err(Error::NotFound))
    }

    pub(crate) fn set_raw(&self, attribute: u16, input: &[u8]) -> Result<(), Error> {
        self.with_attribute(attribute, |att| {
            let capacity = self.capacity(attribute, att);
            if !Self::is_variable_len(att) && capacity != input.len() {
                Err(Error::UnexpectedDataLength {
                    expected: capacity,
                    actual: input.len(),
                })
            } else {
                self.write_value(att, 0, input).map_err(Into::into)
            }
        })
        .unwrap_or(Err(Error::NotFound))
    }

    /// Get the number of attributes in the table
    pub fn len(&self) -> usize {
        self.attributes.len()
    }

    /// Returns true if the table is empty
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
    }

    /// Set the value of a characteristic
    ///
    /// For fixed-length values, the provided data must exactly match the storage size.
    /// For variable-length values, any length up to the configured capacity is accepted.
    ///
    /// Returns an error if the characteristic cannot be found, if the input length is
    /// incompatible with the characteristic storage, or if the data shape does not
    /// match the characteristic type.
    pub fn set<T: AttributeHandle>(&self, attribute_handle: &T, input: &T::Value) -> Result<(), Error> {
        let gatt_value = input.as_gatt();
        self.set_raw(attribute_handle.handle(), gatt_value)
    }

    /// Read the value of the characteristic and pass the value to the provided closure.
    ///
    /// The return value of the closure is returned in this function and is assumed to be infallible.
    ///
    /// If the characteristic for the handle cannot be found, an error is returned.
    pub fn get<T: AttributeHandle<Value = V>, V: FromGatt>(&self, attribute_handle: &T) -> Result<T::Value, Error> {
        let handle = attribute_handle.handle();
        self.with_attribute(handle, |att| {
            self.with_value(handle, att, |value| {
                let value_slice = value.ok_or(Error::NotSupported)?;
                T::Value::from_gatt(value_slice).map_err(|_| {
                    let mut invalid_data = [0u8; MAX_INVALID_DATA_LEN];
                    let len_to_copy = value_slice.len().min(MAX_INVALID_DATA_LEN);
                    invalid_data[..len_to_copy].copy_from_slice(&value_slice[..len_to_copy]);

                    Error::CannotConstructGattValue(invalid_data)
                })
            })
        })
        .unwrap_or(Err(Error::NotFound))
    }

    /// Return the characteristic which corresponds to the supplied value handle
    ///
    /// If no characteristic corresponding to the given value handle was found, returns an error
    pub fn find_characteristic_by_value_handle<T: AsGatt>(&self, handle: u16) -> Result<Characteristic<T>, Error> {
        if handle == 0 {
            return Err(Error::NotFound);
        }

        self.iterate_from(handle - 1, |mut it| {
            if let Some((_, att)) = it.next() {
                if let AttributeData::Declaration { props } = att.data {
                    if let Some((_, value)) = it.next() {
                        let uuid = value.uuid();
                        let end_handle = it.characteristic_group_end();
                        let cccd_handle = it.next().and_then(|(handle, att)| {
                            (att.uuid() == CLIENT_CHARACTERISTIC_CONFIGURATION.into()).then_some(handle)
                        });

                        return Ok(Characteristic {
                            handle,
                            cccd_handle,
                            end_handle,
                            props,
                            uuid,
                            phantom: PhantomData,
                        });
                    }
                }
            }
            Err(Error::NotFound)
        })
    }

    #[cfg(feature = "security")]
    /// Calculate the database hash for the attribute table.
    ///
    /// See Core Specification Vol 3, Part G, Section 7.3.1
    pub fn hash(&self) -> u128 {
        use bt_hci::uuid::*;

        use crate::security_manager::crypto::AesCmac;

        const PRIMARY_SERVICE: Uuid = Uuid::Uuid16(declarations::PRIMARY_SERVICE.to_le_bytes());
        const SECONDARY_SERVICE: Uuid = Uuid::Uuid16(declarations::SECONDARY_SERVICE.to_le_bytes());
        const INCLUDED_SERVICE: Uuid = Uuid::Uuid16(declarations::INCLUDE.to_le_bytes());
        const CHARACTERISTIC: Uuid = Uuid::Uuid16(declarations::CHARACTERISTIC.to_le_bytes());
        const CHARACTERISTIC_EXTENDED_PROPERTIES: Uuid =
            Uuid::Uuid16(descriptors::CHARACTERISTIC_EXTENDED_PROPERTIES.to_le_bytes());

        const CHARACTERISTIC_USER_DESCRIPTION: Uuid =
            Uuid::Uuid16(descriptors::CHARACTERISTIC_USER_DESCRIPTION.to_le_bytes());
        const CLIENT_CHARACTERISTIC_CONFIGURATION: Uuid =
            Uuid::Uuid16(descriptors::CLIENT_CHARACTERISTIC_CONFIGURATION.to_le_bytes());
        const SERVER_CHARACTERISTIC_CONFIGURATION: Uuid =
            Uuid::Uuid16(descriptors::SERVER_CHARACTERISTIC_CONFIGURATION.to_le_bytes());
        const CHARACTERISTIC_PRESENTATION_FORMAT: Uuid =
            Uuid::Uuid16(descriptors::CHARACTERISTIC_PRESENTATION_FORMAT.to_le_bytes());
        const CHARACTERISTIC_AGGREGATE_FORMAT: Uuid =
            Uuid::Uuid16(descriptors::CHARACTERISTIC_AGGREGATE_FORMAT.to_le_bytes());

        let mut mac = AesCmac::db_hash();

        self.iterate(|it| {
            for (handle, att) in it {
                match att.uuid() {
                    PRIMARY_SERVICE
                    | SECONDARY_SERVICE
                    | INCLUDED_SERVICE
                    | CHARACTERISTIC
                    | CHARACTERISTIC_EXTENDED_PROPERTIES => {
                        mac.update(handle.to_le_bytes()).update(att.uuid().as_raw());
                        self.with_value(handle, att, |value| {
                            mac.update(value.unwrap_or(&[]));
                        });
                    }
                    CHARACTERISTIC_USER_DESCRIPTION
                    | CLIENT_CHARACTERISTIC_CONFIGURATION
                    | SERVER_CHARACTERISTIC_CONFIGURATION
                    | CHARACTERISTIC_PRESENTATION_FORMAT
                    | CHARACTERISTIC_AGGREGATE_FORMAT => {
                        mac.update(handle.to_le_bytes()).update(att.uuid().as_raw());
                    }
                    _ => {}
                }
            }
        });

        mac.finalize()
    }
}

/// BattleTime patch: builds a server's attribute table in a constant, so the table is placed in flash.
///
/// Each value that changes at run time gets its place in the table's RAM, counted in `stored_bytes`.
pub struct AttributeTableBuilder<const N: usize> {
    attributes: [Attribute; N],
    length: usize,
    stored_bytes: usize,
}

impl<const N: usize> Default for AttributeTableBuilder<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> AttributeTableBuilder<N> {
    /// An empty table with room for exactly `N` attributes.
    pub const fn new() -> Self {
        Self {
            attributes: [Attribute::UNUSED; N],
            length: 0,
            stored_bytes: 0,
        }
    }

    const fn push(&mut self, attribute: Attribute) -> u16 {
        assert!(
            self.length < N,
            "the attribute table has more attributes than it counted"
        );
        self.attributes[self.length] = attribute;
        self.length += 1;
        self.length as u16
    }

    const fn next_handle(&self) -> u16 {
        self.length as u16 + 1
    }

    const fn reserve_stored(
        &mut self,
        capacity: usize,
        variable_len: bool,
        initial: Option<InitialValue>,
    ) -> AttributeData {
        let offset = self.stored_bytes;
        let reserved = if variable_len {
            capacity + STORED_LENGTH_BYTES
        } else {
            capacity
        };
        assert!(
            offset + reserved <= u16::MAX as usize,
            "the stored values are too large"
        );
        self.stored_bytes += reserved;
        AttributeData::Stored {
            offset: offset as u16,
            capacity: capacity as u16,
            variable_len,
            initial,
        }
    }

    /// Adds a primary service declaration and returns its handle.
    pub const fn add_service(&mut self, uuid: Uuid) -> u16 {
        self.push(Attribute {
            stored_uuid: uuid,
            permissions: AttPermissions::read_only(),
            data: AttributeData::Service { secondary: false },
        })
    }

    /// Adds a secondary service declaration and returns its handle.
    pub const fn add_secondary_service(&mut self, uuid: Uuid) -> u16 {
        self.push(Attribute {
            stored_uuid: uuid,
            permissions: AttPermissions::read_only(),
            data: AttributeData::Service { secondary: true },
        })
    }

    const fn add_characteristic_with<T: AsGatt + ?Sized>(
        &mut self,
        uuid: Uuid,
        props: CharacteristicProps,
        permissions: AttPermissions,
        cccd_write: PermissionLevel,
        data: AttributeData,
    ) -> Characteristic<T> {
        self.push(Attribute {
            stored_uuid: Uuid::Uuid16(CHARACTERISTIC.to_le_bytes()),
            permissions: AttPermissions::read_only(),
            data: AttributeData::Declaration { props },
        });
        let handle = self.push(Attribute {
            stored_uuid: uuid,
            permissions,
            data,
        });
        let cccd_handle = if props.has_cccd() {
            Some(self.push(Attribute {
                stored_uuid: Uuid::Uuid16(CLIENT_CHARACTERISTIC_CONFIGURATION.to_le_bytes()),
                permissions: AttPermissions::new(PermissionLevel::Allowed, cccd_write),
                data: AttributeData::ClientSpecific {
                    variable_len: false,
                    capacity: 2,
                },
            }))
        } else {
            None
        };
        Characteristic {
            handle,
            cccd_handle,
            end_handle: self.next_handle() - 1,
            props,
            uuid,
            phantom: PhantomData,
        }
    }

    /// Adds a characteristic whose value is kept in RAM, with a CCCD when it notifies or indicates.
    ///
    /// The value starts from what `initial` writes, or from zero bytes. Call [`Self::finish_characteristic`]
    /// after its descriptors.
    pub const fn add_characteristic<T: AsGatt + ?Sized>(
        &mut self,
        uuid: Uuid,
        props: CharacteristicProps,
        permissions: AttPermissions,
        cccd_write: PermissionLevel,
        initial: Option<InitialValue>,
    ) -> Characteristic<T> {
        assert!(
            T::MAX_SIZE <= u16::MAX as usize,
            "a stored characteristic must fit in 64 KB"
        );
        let data = self.reserve_stored(T::MAX_SIZE, T::MIN_SIZE != T::MAX_SIZE, initial);
        self.add_characteristic_with(uuid, props, permissions, cccd_write, data)
    }

    /// Adds a read-only characteristic whose value is fixed when the firmware is compiled.
    pub const fn add_constant_characteristic<T: AsGatt + ?Sized>(
        &mut self,
        uuid: Uuid,
        read: PermissionLevel,
        value: fn() -> &'static [u8],
    ) -> Characteristic<T> {
        self.add_characteristic_with(
            uuid,
            CharacteristicProps(CharacteristicProp::Read as u8),
            AttPermissions::new(read, PermissionLevel::NotAllowed),
            PermissionLevel::NotAllowed,
            AttributeData::Constant { value },
        )
    }

    /// Adds a read-only characteristic whose value is lent when the server is built, into `slot`.
    pub(crate) const fn add_borrowed_characteristic<T: AsGatt + ?Sized>(
        &mut self,
        uuid: Uuid,
        slot: u8,
    ) -> Characteristic<T> {
        assert!(
            (slot as usize) < BORROWED_VALUE_SLOTS,
            "there is no such borrowed value slot"
        );
        self.add_characteristic_with(
            uuid,
            CharacteristicProps(CharacteristicProp::Read as u8),
            AttPermissions::read_only(),
            PermissionLevel::NotAllowed,
            AttributeData::Borrowed { slot },
        )
    }

    /// Adds a descriptor whose value is kept in RAM.
    ///
    /// Its capacity follows TrouBLE's builder: the value type's size, at most
    /// [`MAX_SMALL_DATA_SIZE`], when `capacity_hint` is that small, and `capacity_hint` otherwise.
    pub const fn add_descriptor<T: AsGatt + ?Sized>(
        &mut self,
        uuid: Uuid,
        permissions: AttPermissions,
        capacity_hint: usize,
        initial: Option<InitialValue>,
    ) -> Descriptor<T> {
        let capacity = if capacity_hint <= MAX_SMALL_DATA_SIZE {
            if T::MAX_SIZE < MAX_SMALL_DATA_SIZE {
                T::MAX_SIZE
            } else {
                MAX_SMALL_DATA_SIZE
            }
        } else {
            capacity_hint
        };
        let data = self.reserve_stored(capacity, T::MIN_SIZE != T::MAX_SIZE, initial);
        let handle = self.push(Attribute {
            stored_uuid: uuid,
            permissions,
            data,
        });
        Descriptor {
            handle,
            uuid,
            phantom: PhantomData,
        }
    }

    /// Adds a descriptor kept in RAM whose value type is that of `_value`, for descriptors declared without a type.
    pub const fn add_descriptor_like<T: AsGatt + ?Sized>(
        &mut self,
        _value: &T,
        uuid: Uuid,
        permissions: AttPermissions,
        capacity_hint: usize,
        initial: Option<InitialValue>,
    ) -> Descriptor<T> {
        self.add_descriptor(uuid, permissions, capacity_hint, initial)
    }

    /// Adds a read-only descriptor whose value is fixed when the firmware is compiled.
    pub const fn add_constant_descriptor<T: AsGatt + ?Sized>(
        &mut self,
        uuid: Uuid,
        read: PermissionLevel,
        value: fn() -> &'static [u8],
    ) -> Descriptor<T> {
        let handle = self.push(Attribute {
            stored_uuid: uuid,
            permissions: AttPermissions::new(read, PermissionLevel::NotAllowed),
            data: AttributeData::Constant { value },
        });
        Descriptor {
            handle,
            uuid,
            phantom: PhantomData,
        }
    }

    /// Ends a characteristic after its descriptors, so its end handle covers them.
    pub const fn finish_characteristic<T: AsGatt + ?Sized>(
        &self,
        characteristic: Characteristic<T>,
    ) -> Characteristic<T> {
        Characteristic {
            end_handle: self.next_handle() - 1,
            ..characteristic
        }
    }

    /// The RAM the table's stored values need, in bytes.
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// The finished attributes, which must be exactly `N`.
    pub const fn finish(self) -> [Attribute; N] {
        assert!(
            self.length == N,
            "the attribute table has fewer attributes than it counted"
        );
        self.attributes
    }
}

/// A type which holds a handle to an attribute in the attribute table
pub trait AttributeHandle {
    /// The data type which the attribute contains
    type Value: AsGatt + ?Sized;

    /// Returns the attribute's handle
    fn handle(&self) -> u16;
}

impl<T: AsGatt + ?Sized> AttributeHandle for Characteristic<T> {
    type Value = T;

    fn handle(&self) -> u16 {
        self.handle
    }
}

/// Invalid handle value
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct InvalidHandle;

impl core::fmt::Display for InvalidHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

impl core::error::Error for InvalidHandle {}

impl From<InvalidHandle> for Error {
    fn from(value: InvalidHandle) -> Self {
        Error::InvalidValue
    }
}
/// A characteristic in the attribute table.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Characteristic<T: AsGatt + ?Sized> {
    /// Handle value assigned to the Client Characteristic Configuration Descriptor (if any)
    pub cccd_handle: Option<u16>,
    /// Handle value assigned to this characteristic when it is added to the Gatt Attribute Table
    pub handle: u16,
    /// Last attribute handle belonging to this characteristic (value handle + descriptors)
    pub end_handle: u16,
    /// Properties of this characteristic
    pub props: CharacteristicProps,
    /// UUID of this characteristic
    pub uuid: Uuid,
    pub(crate) phantom: PhantomData<T>,
}

impl<T: AsGatt + ?Sized> Characteristic<T> {
    /// Check if notifications should be sent to `connection` for this characteristic.
    pub fn should_notify<P: PacketPool>(&self, connection: &GattConnection<'_, '_, P>) -> bool {
        let Some(cccd_handle) = self.cccd_handle else {
            return false;
        };
        connection.server.should_notify(connection.raw(), cccd_handle)
    }

    /// Send a notification to `connection` with a new `value` for the characteristic.
    ///
    /// If `store` is true, the new value will be written to the table before sending the notification.
    ///
    /// If the provided connection has not subscribed for this characteristic, it will not be notified.
    ///
    /// If the characteristic does not support notifications, an error is returned.
    pub async fn notify<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        value: &T,
        store: bool,
    ) -> Result<(), Error> {
        self.notify_raw(connection, value.as_gatt(), store).await
    }

    /// Send a notification to `connection` with a new `value` for the characteristic.
    ///
    /// If `store` is true, the new value will be written to the table before sending the notification.
    ///
    /// If the provided connection has not subscribed for this characteristic, it will not be notified.
    ///
    /// If the characteristic does not support notifications, an error is returned.
    pub async fn notify_raw<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        value: &[u8],
        store: bool,
    ) -> Result<(), Error> {
        let server = connection.server;
        if store {
            server.set(connection.raw(), self.handle, value)?;
        }

        let cccd_handle = self.cccd_handle.ok_or(Error::NotFound)?;
        let conn = connection.raw();
        if !server.should_notify(conn, cccd_handle) {
            // No reason to fail?
            return Ok(());
        }

        self.authorize_unsolicited(connection, cccd_handle).await?;

        let uns = AttUns::Notify {
            handle: self.handle,
            data: value,
        };
        let pdu = gatt::assemble(conn, crate::att::AttServer::Unsolicited(uns))?;
        conn.send(pdu).await;
        Ok(())
    }

    /// Check if indications should be sent to `connection` for this characteristic.
    pub fn should_indicate<P: PacketPool>(&self, connection: &GattConnection<'_, '_, P>) -> bool {
        let Some(cccd_handle) = self.cccd_handle else {
            return false;
        };
        connection.server.should_notify(connection.raw(), cccd_handle)
    }

    /// Send an indication to `connection` with a new `value` for the characteristic.
    ///
    /// If `store` is true, the new value will be written to the table before sending the indication.
    ///
    /// If the provided connection has not subscribed for this characteristic, it will not be sent an indication.
    ///
    /// If the characteristic does not support indications, an error is returned.
    ///
    /// Blocks until the peer's `HandleValueConfirmation` is received, for up to 30 seconds
    /// (BT Core Spec Vol 3, Part F, Section 3.3.3 ATT transaction timeout). On timeout the
    /// connection is closed and `Error::Timeout` is returned. Concurrent calls on the same
    /// connection are serialized; callers do not need to coordinate. Returns
    /// `Error::Disconnected` if the connection is dropped while waiting.
    pub async fn indicate<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        value: &T,
        store: bool,
    ) -> Result<(), Error> {
        self.indicate_raw(connection, value.as_gatt(), store).await
    }

    /// Send an indication to `connection` with a new `value` for the characteristic.
    ///
    /// If `store` is true, the new value will be written to the table before sending the indication.
    ///
    /// If the provided connection has not subscribed for this characteristic, it will not be sent an indication.
    ///
    /// If the characteristic does not support indications, an error is returned.
    ///
    /// Blocks until the peer's `HandleValueConfirmation` is received, for up to 30 seconds
    /// (BT Core Spec Vol 3, Part F, Section 3.3.3 ATT transaction timeout). On timeout the
    /// connection is closed and `Error::Timeout` is returned. Concurrent calls on the same
    /// connection are serialized; callers do not need to coordinate. Returns
    /// `Error::Disconnected` if the connection is dropped while waiting.
    pub async fn indicate_raw<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        value: &[u8],
        store: bool,
    ) -> Result<(), Error> {
        let server = connection.server;
        if store {
            server.set(connection.raw(), self.handle, value)?;
        }

        let cccd_handle = self.cccd_handle.ok_or(Error::NotFound)?;
        let conn = connection.raw();
        if !server.should_indicate(conn, cccd_handle) {
            // No reason to fail?
            return Ok(());
        }

        self.authorize_unsolicited(connection, cccd_handle).await?;

        conn.acquire_indication_slot().await?;
        let _drop = crate::host::OnDrop::new(|| conn.release_indication_slot());

        let uns = AttUns::Indicate {
            handle: self.handle,
            data: value,
        };
        let pdu = gatt::assemble(conn, crate::att::AttServer::Unsolicited(uns))?;
        conn.send(pdu).await;

        match with_timeout(ATT_TRANSACTION_TIMEOUT, conn.wait_indication_confirmation()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                warn!("[gatt] ATT transaction timeout (30s) waiting for indication confirmation, disconnecting");
                conn.disconnect();
                Err(Error::Timeout)
            }
        }
    }

    /// Set the value of the characteristic in the provided attribute server.
    pub fn set<M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
        value: &T,
    ) -> Result<(), Error> {
        let value = value.as_gatt();
        server.table().set_raw(self.handle, value)?;
        Ok(())
    }

    /// Set the value of the characteristic in the provided attribute server.
    pub fn set_ro<'a, M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'a, M, P, AT, CN>,
        value: &'a T,
    ) -> Result<(), Error> {
        let value = value.as_gatt();
        server.table().set_ro(self.handle, value)?;
        Ok(())
    }

    /// Read the value of the characteristic.
    ///
    /// If the characteristic for the handle cannot be found, an error is returned.
    ///
    pub fn get<M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
    ) -> Result<T, Error>
    where
        T: FromGatt,
    {
        server.table().get(self)
    }

    /// Access the raw byte value of the characteristic.
    pub fn with_raw_value<M: RawMutex, P: PacketPool, R, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        let table = server.table();
        table
            .with_attribute(self.handle, |att| {
                table.with_value(self.handle, att, |value| value.map(f))
            })
            .flatten()
    }

    /// Returns the attribute handle for the characteristic's client characteristic configuration descriptor (if available)
    pub fn cccd_handle(&self) -> Option<CharacteristicPropertiesHandle> {
        self.cccd_handle.map(CharacteristicPropertiesHandle)
    }

    /// Convert this characteristic's type to raw bytes
    pub fn to_raw(self) -> Characteristic<[u8]> {
        Characteristic {
            cccd_handle: self.cccd_handle,
            handle: self.handle,
            end_handle: self.end_handle,
            props: self.props,
            uuid: self.uuid,
            phantom: PhantomData,
        }
    }

    async fn authorize_unsolicited<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        cccd_handle: u16,
    ) -> Result<(), Error> {
        let server = connection.server;
        let conn = connection.raw();

        // Ensure encryption before sending notifications that require security
        // (BT Core Spec Vol 3, Part C, Section 10.3.1.1: server "shall" initiate encryption)
        // Write permission on the CCCD defines permission to receive notifications/indications
        match server.can_write(conn, cccd_handle) {
            Ok(()) => Ok(()),
            Err(err) => {
                #[cfg(feature = "security")]
                if conn.is_bonded_peer() && !conn.security_level().is_ok_and(|l| l.encrypted()) {
                    conn.try_enable_encryption().await?;
                    server.can_write(conn, cccd_handle)?;
                    return Ok(());
                }
                Err(err.into())
            }
        }
    }
}

/// Attribute handle for a characteristic's properties
pub struct CharacteristicPropertiesHandle(u16);

impl AttributeHandle for CharacteristicPropertiesHandle {
    type Value = CharacteristicProps;

    fn handle(&self) -> u16 {
        self.0
    }
}
/// Characteristic descriptor handle.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug)]
pub struct Descriptor<T: AsGatt + ?Sized> {
    pub(crate) handle: u16,
    pub(crate) uuid: Uuid,
    pub(crate) phantom: PhantomData<T>,
}

impl<T: AsGatt + ?Sized> AttributeHandle for Descriptor<T> {
    type Value = T;

    fn handle(&self) -> u16 {
        self.handle
    }
}

impl<T: AsGatt + ?Sized> Descriptor<T> {
    /// Get the handle of this descriptor.
    pub fn handle(&self) -> u16 {
        self.handle
    }

    /// Get the UUID of this descriptor.
    pub fn uuid(&self) -> &Uuid {
        &self.uuid
    }

    /// Set the value of the descriptor in the provided attribute server.
    pub fn set<M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
        value: &T,
    ) -> Result<(), Error> {
        let value = value.as_gatt();
        server.table().set_raw(self.handle, value)?;
        Ok(())
    }

    /// Read the value of the descriptor.
    ///
    /// If the descriptor for the handle cannot be found, an error is returned.
    ///
    pub fn get<M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
    ) -> Result<T, Error>
    where
        T: FromGatt,
    {
        server.table().get(self)
    }
}

/// Iterator over attributes.
pub struct AttributeIterator<'a> {
    attributes: &'a [Attribute],
    pos: usize,
}

impl<'a> Iterator for AttributeIterator<'a> {
    type Item = (u16, &'a Attribute);

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.attributes.len() {
            let att = &self.attributes[self.pos];
            self.pos += 1;
            let handle = self.pos as u16;
            Some((handle, att))
        } else {
            None
        }
    }
}

impl AttributeIterator<'_> {
    /// Find the handle of the last attribute in the current GATT service.
    ///
    /// Returns `u16::MAX` for the last service in the table.
    pub fn service_group_end(&self) -> u16 {
        // We return the 0-based index of the next service definition. When interpretted as a 1-based handle,
        // this represents the handle of the last attribute before the next service definition.
        self.attributes
            .iter()
            .enumerate()
            .skip(self.pos)
            .find(|(_, attr)| matches!(attr.data, AttributeData::Service { .. }))
            .map(|(i, _)| i as u16)
            .unwrap_or(u16::MAX)
    }

    /// Find the handle of the last attribute in the current GATT characteristic.
    pub fn characteristic_group_end(&self) -> u16 {
        // We return the 0-based index of the next characteristic or service definition. When interpretted as a 1-based handle,
        // this represents the handle of the last attribute before the next characteristic or service definition.
        self.attributes
            .iter()
            .enumerate()
            .skip(self.pos)
            .find(|(_, attr)| {
                matches!(
                    attr.data,
                    AttributeData::Service { .. } | AttributeData::Declaration { .. }
                )
            })
            .map(|(i, _)| i as u16)
            .unwrap_or(self.attributes.len() as u16)
    }
}
/// Properties of a characteristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CharacteristicProps(u8);

impl<'a> From<&'a [CharacteristicProp]> for CharacteristicProps {
    fn from(props: &'a [CharacteristicProp]) -> Self {
        let mut val: u8 = 0;
        for prop in props {
            val |= *prop as u8;
        }
        CharacteristicProps(val)
    }
}

impl<'a, const N: usize> From<&'a [CharacteristicProp; N]> for CharacteristicProps {
    fn from(props: &'a [CharacteristicProp; N]) -> Self {
        let mut val: u8 = 0;
        for prop in props {
            val |= *prop as u8;
        }
        CharacteristicProps(val)
    }
}

impl<const N: usize> From<[CharacteristicProp; N]> for CharacteristicProps {
    fn from(props: [CharacteristicProp; N]) -> Self {
        let mut val: u8 = 0;
        for prop in props {
            val |= prop as u8;
        }
        CharacteristicProps(val)
    }
}

impl From<u8> for CharacteristicProps {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl CharacteristicProps {
    /// Check if any of the properties are set.
    pub fn any(&self, props: &[CharacteristicProp]) -> bool {
        for p in props {
            if (*p as u8) & self.0 != 0 {
                return true;
            }
        }
        false
    }

    pub(crate) fn default_permissions(&self) -> AttPermissions {
        let read = if (self.0 & CharacteristicProp::Read as u8) != 0 {
            PermissionLevel::Allowed
        } else {
            PermissionLevel::NotAllowed
        };

        let write = if (self.0
            & (CharacteristicProp::Write as u8
                | CharacteristicProp::WriteWithoutResponse as u8
                | CharacteristicProp::AuthenticatedWrite as u8))
            != 0
        {
            PermissionLevel::Allowed
        } else {
            PermissionLevel::NotAllowed
        };

        AttPermissions {
            read,
            write,
            #[cfg(feature = "legacy-pairing")]
            min_key_len: 0,
        }
    }

    /// Check if the characteristic will have a Client Characteristic Configuration Descriptor
    pub const fn has_cccd(&self) -> bool {
        (self.0 & (CharacteristicProp::Indicate as u8 | CharacteristicProp::Notify as u8)) != 0
    }

    /// Get the raw value of the characteristic props
    pub const fn to_raw(self) -> u8 {
        self.0
    }

    /// BattleTime patch: properties from their raw bits, for tables built in a constant.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }
}

impl FixedGattValue for CharacteristicProps {
    const SIZE: usize = 1;
}

impl FromGatt for CharacteristicProps {
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
        if data.len() != Self::SIZE {
            return Err(FromGattError::InvalidLength);
        }

        Ok(CharacteristicProps(data[0]))
    }
}

impl AsGatt for CharacteristicProps {
    const MIN_SIZE: usize = Self::SIZE;
    const MAX_SIZE: usize = Self::SIZE;

    fn as_gatt(&self) -> &[u8] {
        AsGatt::as_gatt(&self.0)
    }
}

/// CCCD flag values.
#[derive(Clone, Copy)]
pub enum CCCDFlag {
    /// Notifications enabled.
    Notify = 0x1,
    /// Indications enabled.
    Indicate = 0x2,
}

/// CCCD flag.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct CCCD(pub(crate) u16);

impl<const T: usize> From<[CCCDFlag; T]> for CCCD {
    fn from(props: [CCCDFlag; T]) -> Self {
        let mut val: u16 = 0;
        for prop in props {
            val |= prop as u16;
        }
        CCCD(val)
    }
}

impl From<u16> for CCCD {
    fn from(value: u16) -> Self {
        CCCD(value)
    }
}

impl CCCD {
    /// Get raw value
    pub fn raw(&self) -> u16 {
        self.0
    }

    /// Clear all properties
    pub fn disable(&mut self) {
        self.0 = 0;
    }

    /// Check if any of the properties are set.
    pub fn any(&self, props: &[CCCDFlag]) -> bool {
        for p in props {
            if (*p as u16) & self.0 != 0 {
                return true;
            }
        }
        false
    }

    /// Enable or disable notifications
    pub fn set_notify(&mut self, is_enabled: bool) {
        let mask: u16 = CCCDFlag::Notify as u16;
        self.0 = if is_enabled { self.0 | mask } else { self.0 & !mask };
    }

    /// Check if notifications are enabled
    pub fn should_notify(&self) -> bool {
        (self.0 & (CCCDFlag::Notify as u16)) != 0
    }

    /// Enable or disable indication
    pub fn set_indicate(&mut self, is_enabled: bool) {
        let mask: u16 = CCCDFlag::Indicate as u16;
        self.0 = if is_enabled { self.0 | mask } else { self.0 & !mask };
    }

    /// Check if indications are enabled
    pub fn should_indicate(&self) -> bool {
        (self.0 & (CCCDFlag::Indicate as u16)) != 0
    }
}
/// Turns the attributes a builder holds so far into a table, leaking them and their RAM as a firmware's statics are.
#[cfg(test)]
pub(crate) fn test_table<M: RawMutex, const N: usize>(
    builder: AttributeTableBuilder<N>,
) -> AttributeTable<'static, M, N> {
    extern crate std;
    let stored = std::boxed::Box::leak(std::vec![0u8; builder.stored_bytes].into_boxed_slice());
    let attributes = std::boxed::Box::leak(std::boxed::Box::new(builder.attributes));
    AttributeTable::new(&attributes[..builder.length], stored, [&[]; BORROWED_VALUE_SLOTS])
}

#[cfg(test)]
mod tests {
    extern crate std;

    use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    use heapless::{String, Vec};

    use super::*;

    const READ_AND_WRITE: CharacteristicProps =
        CharacteristicProps::from_bits(CharacteristicProp::Read as u8 | CharacteristicProp::Write as u8);
    const READ_AND_WRITE_PERMISSIONS: AttPermissions =
        AttPermissions::new(PermissionLevel::Allowed, PermissionLevel::Allowed);

    fn table_of<const N: usize>(builder: AttributeTableBuilder<N>) -> AttributeTable<'static, NoopRawMutex, N> {
        test_table(builder)
    }

    #[cfg(feature = "security")]
    #[test]
    fn database_hash() {
        use bt_hci::uuid::characteristic::{
            APPEARANCE, CLIENT_SUPPORTED_FEATURES, DATABASE_HASH, DEVICE_NAME, SERVICE_CHANGED,
        };
        use bt_hci::uuid::descriptors::{CHARACTERISTIC_PRESENTATION_FORMAT, CHARACTERISTIC_USER_DESCRIPTION};
        use bt_hci::uuid::service::{GAP, GATT};

        // The message hashed for this table, and how to check it, are in upstream's version of this test.
        const CUSTOM_SERVICE: u128 = 0x12345678_12345678_12345678_9abcdef0;
        const CUSTOM_CHARACTERISTIC: u128 = 0x12345678_12345678_12345678_9abcdef1;
        fn empty() -> &'static [u8] {
            b""
        }
        fn description() -> &'static [u8] {
            b"Custom Characteristic"
        }
        fn presentation_format() -> &'static [u8] {
            &[4, 0, 0, 0x27, 1, 0, 0]
        }
        let read = PermissionLevel::Allowed;

        let mut builder = AttributeTableBuilder::<19>::new();
        builder.add_service(GAP.into());
        builder.add_constant_characteristic::<[u8]>(DEVICE_NAME.into(), read, empty);
        builder.add_constant_characteristic::<[u8]>(APPEARANCE.into(), read, empty);
        builder.add_service(GATT.into());
        builder.add_characteristic::<[u8; 0]>(
            SERVICE_CHANGED.into(),
            CharacteristicProps::from_bits(CharacteristicProp::Indicate as u8),
            AttPermissions::read_only(),
            PermissionLevel::Allowed,
            None,
        );
        builder.add_characteristic::<[u8; 0]>(
            CLIENT_SUPPORTED_FEATURES.into(),
            READ_AND_WRITE,
            AttPermissions::read_only(),
            PermissionLevel::NotAllowed,
            None,
        );
        builder.add_constant_characteristic::<[u8]>(DATABASE_HASH.into(), read, empty);
        builder.add_service(Uuid::from(CUSTOM_SERVICE));
        builder.add_characteristic::<[u8; 0]>(
            Uuid::from(CUSTOM_CHARACTERISTIC),
            CharacteristicProps::from_bits(CharacteristicProp::Notify as u8 | CharacteristicProp::Read as u8),
            AttPermissions::read_only(),
            PermissionLevel::Allowed,
            None,
        );
        builder.add_constant_descriptor::<[u8]>(CHARACTERISTIC_USER_DESCRIPTION.into(), read, description);
        builder.add_constant_descriptor::<[u8]>(CHARACTERISTIC_PRESENTATION_FORMAT.into(), read, presentation_format);
        let table = table_of(builder);

        let expected = 0xc7352cced28d6608d4b057d247d8be76;
        assert_eq!(table.hash(), expected);
    }

    #[test]
    fn set_updates_variable_length_when_value_fills_backing_storage() {
        let mut builder = AttributeTableBuilder::<3>::new();
        builder.add_service(Uuid::new_long([0x10; 16]));
        let characteristic = builder.add_characteristic::<Vec<u8, 4>>(
            Uuid::new_long([0x11; 16]),
            READ_AND_WRITE,
            READ_AND_WRITE_PERMISSIONS,
            PermissionLevel::NotAllowed,
            Some(|buffer| write_initial_value(buffer, &Vec::<u8, 4>::from_slice(b"ab").unwrap())),
        );
        let table = table_of(builder);

        let replacement = Vec::<u8, 4>::from_slice(b"wxyz").unwrap();
        table.set(&characteristic, &replacement).unwrap();

        let stored: Vec<u8, 4> = table.get(&characteristic).unwrap();
        assert_eq!(stored.as_slice(), b"wxyz");
    }

    #[test]
    fn set_updates_small_variable_length_when_value_reaches_capacity() {
        let mut builder = AttributeTableBuilder::<3>::new();
        builder.add_service(Uuid::new_long([0x12; 16]));
        let characteristic = builder.add_characteristic::<String<8>>(
            Uuid::new_long([0x13; 16]),
            READ_AND_WRITE,
            READ_AND_WRITE_PERMISSIONS,
            PermissionLevel::NotAllowed,
            Some(|buffer| write_initial_value(buffer, &String::<8>::try_from("hi").unwrap())),
        );
        let table = table_of(builder);

        let replacement = String::<8>::try_from("12345678").unwrap();
        table.set(&characteristic, &replacement).unwrap();

        let stored: String<8> = table.get(&characteristic).unwrap();
        assert_eq!(stored.as_str(), "12345678");
    }

    #[test]
    fn a_declaration_names_the_next_handle_and_the_whole_uuid() {
        let uuid = [0x21; 16];
        let mut builder = AttributeTableBuilder::<3>::new();
        builder.add_service(Uuid::new_short(0x180f));
        let characteristic =
            builder.add_constant_characteristic::<[u8]>(Uuid::new_long(uuid), PermissionLevel::Allowed, || b"fixed");
        let table = table_of(builder);

        let mut declaration = [0u8; 32];
        let length = table.read(characteristic.handle - 1, 0, &mut declaration).unwrap();

        assert_eq!(declaration[..3], [0x02, 3, 0]);
        assert_eq!(declaration[3..length], uuid);
        let mut value = [0u8; 8];
        let length = table.read(characteristic.handle, 0, &mut value).unwrap();
        assert_eq!(&value[..length], b"fixed");
        assert!(table.write(characteristic.handle, 0, b"x").is_err());
    }
}
