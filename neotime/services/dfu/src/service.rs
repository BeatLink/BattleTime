//! The GATT service for TrouBLE: three characteristics under the legacy DFU service UUID.

use trouble_host::prelude::*;

use crate::PROTOCOL_VERSION;

/// The phone writes commands to the control point, streams the image through the packet
/// characteristic, and reads the version to recognise the protocol.
#[gatt_service(uuid = "00001530-1212-efde-1523-785feabcd123")]
pub struct LegacyDfuService {
    #[characteristic(uuid = "00001531-1212-efde-1523-785feabcd123", write, notify)]
    pub control_point: [u8; 20],
    #[characteristic(uuid = "00001532-1212-efde-1523-785feabcd123", write_without_response)]
    pub packet: [u8; 20],
    #[characteristic(uuid = "00001534-1212-efde-1523-785feabcd123", read, value = &PROTOCOL_VERSION)]
    pub version: &'static [u8; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DfuWrite {
    ControlPoint,
    Packet,
}

impl LegacyDfuService {
    /// Tells which of the service's characteristics a write landed on, if any.
    pub fn classify(&self, handle: u16) -> Option<DfuWrite> {
        if handle == self.control_point.handle {
            Some(DfuWrite::ControlPoint)
        } else if handle == self.packet.handle {
            Some(DfuWrite::Packet)
        } else {
            None
        }
    }
}
