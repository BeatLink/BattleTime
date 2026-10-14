//! Ties the host interface and the link layer to a radio, and offers them to TrouBLE as an HCI transport.

use core::cell::RefCell;
use core::future::poll_fn;
use core::task::Poll;

use bt_hci::transport::{PacketKind as TransportPacketKind, PacketToController, PacketToHost, Transport};
use critical_section::Mutex;
use embassy_sync::waitqueue::AtomicWaker;

use crate::host_interface::{HostInterface, HostPacketError, MAXIMUM_HOST_PACKET_LENGTH};
use crate::link_layer::RadioOperation;
use crate::packets::RADIO_BUFFER_LENGTH;

// ============================================================================
// Radio
// ============================================================================

/// What the controller needs from the radio hardware.
pub trait Radio {
    /// The radio's clock, in microseconds, wrapping round.
    fn now(&self) -> u32;

    /// Stops whatever the radio is doing and starts `operation` instead.
    fn run(&self, operation: RadioOperation);

    /// Lends out the buffer the radio sends from; only called while the radio is idle.
    fn with_transmit_buffer<Result>(&self, use_buffer: impl FnOnce(&mut [u8]) -> Result) -> Result;
}

/// What the radio saw, reported to the controller when it finishes an operation.
#[derive(Clone, Copy, Debug)]
pub enum RadioEvent<'packet> {
    Transmitted {
        end_time: u32,
    },
    Received {
        packet: &'packet [u8],
        crc_is_valid: bool,
        address_time: u32,
        end_time: u32,
        signal_strength: i8,
    },
    NothingReceived {
        now: u32,
    },
}

// ============================================================================
// Shared state
// ============================================================================

/// The controller's state, shared between the radio interrupt and the host's task.
pub struct ControllerState {
    host_interface: Mutex<RefCell<Option<HostInterface>>>,
    host_waker: AtomicWaker,
    room_waker: AtomicWaker,
}

impl ControllerState {
    pub const fn new() -> ControllerState {
        ControllerState {
            host_interface: Mutex::new(RefCell::new(None)),
            host_waker: AtomicWaker::new(),
            room_waker: AtomicWaker::new(),
        }
    }

    pub fn start(&self, host_interface: HostInterface) {
        critical_section::with(|critical_section| {
            self.host_interface
                .borrow_ref_mut(critical_section)
                .replace(host_interface);
        });
    }

    fn with_host_interface<Result>(
        &self,
        use_interface: impl FnOnce(&mut HostInterface) -> Result,
    ) -> Result {
        critical_section::with(|critical_section| {
            let mut host_interface = self.host_interface.borrow_ref_mut(critical_section);
            use_interface(
                host_interface
                    .as_mut()
                    .expect("the controller is started before use"),
            )
        })
    }

    /// True while the radio has nothing to do: not started, or neither advertising nor connected.
    pub fn radio_is_idle(&self) -> bool {
        critical_section::with(|critical_section| {
            self.host_interface
                .borrow_ref(critical_section)
                .as_ref()
                .is_none_or(|host_interface| {
                    !host_interface.link_layer.is_advertising() && !host_interface.link_layer.is_connected()
                })
        })
    }

    /// Ends a connection whose radio has gone quiet past its supervision timeout, and tells the host.
    /// The radio's own events normally do this; a timer calls it in case they have stopped.
    pub fn check_supervision(&self, radio: &impl Radio) {
        critical_section::with(|critical_section| {
            let mut host_interface = self.host_interface.borrow_ref_mut(critical_section);
            let Some(host_interface) = host_interface.as_mut() else {
                return;
            };
            if let Some(operation) = host_interface.link_layer.check_supervision(radio.now()) {
                radio.run(operation);
                self.host_waker.wake();
            }
        });
    }

    /// Hands the link layer what the radio saw, then starts the radio on whatever comes next.
    pub fn on_radio_event(&self, radio: &impl Radio, event: RadioEvent) {
        self.with_host_interface(|host_interface| {
            let link_layer = &mut host_interface.link_layer;
            let room_before = link_layer.outgoing.len();

            let operation = radio.with_transmit_buffer(|transmit_buffer| match event {
                RadioEvent::Transmitted { end_time } => link_layer.on_transmitted(end_time, transmit_buffer),
                RadioEvent::Received {
                    packet,
                    crc_is_valid,
                    address_time,
                    end_time,
                    signal_strength,
                } => link_layer.on_received(
                    packet,
                    crc_is_valid,
                    address_time,
                    end_time,
                    signal_strength,
                    transmit_buffer,
                ),
                RadioEvent::NothingReceived { now } => link_layer.on_nothing_received(now, transmit_buffer),
            });
            radio.run(operation);

            if !link_layer.events.is_empty() || link_layer.completed_packets > 0 {
                self.host_waker.wake();
            }
            if link_layer.outgoing.len() < room_before {
                self.room_waker.wake();
            }
        });
    }
}

impl Default for ControllerState {
    fn default() -> Self {
        ControllerState::new()
    }
}

// ============================================================================
// HCI transport
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// The host sent a packet the controller could not make sense of.
    MalformedPacket,
    /// The host sent a data packet longer than the 27 bytes it was told to fragment at.
    PacketTooLong,
    /// A packet for the host did not fit the buffer the host gave.
    HostBufferTooSmall,
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, formatter)
    }
}

impl core::error::Error for Error {}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Error::MalformedPacket | Error::PacketTooLong => embedded_io::ErrorKind::InvalidData,
            Error::HostBufferTooSmall => embedded_io::ErrorKind::OutOfMemory,
        }
    }
}

impl From<HostPacketError> for Error {
    fn from(error: HostPacketError) -> Self {
        match error {
            HostPacketError::Malformed => Error::MalformedPacket,
            HostPacketError::TooLong => Error::PacketTooLong,
        }
    }
}

impl<ReadError: embedded_io::Error> From<bt_hci::ReadHciError<ReadError>> for Error {
    fn from(error: bt_hci::ReadHciError<ReadError>) -> Self {
        match error {
            bt_hci::ReadHciError::BufferTooSmall => Error::HostBufferTooSmall,
            _ => Error::MalformedPacket,
        }
    }
}

/// The controller as TrouBLE sees it: wrap it in `bt_hci::controller::ExternalController`.
pub struct BluetoothController<'state, RadioDriver: Radio> {
    state: &'state ControllerState,
    radio: RadioDriver,
}

impl<'state, RadioDriver: Radio> BluetoothController<'state, RadioDriver> {
    pub fn new(state: &'state ControllerState, radio: RadioDriver) -> Self {
        BluetoothController { state, radio }
    }
}

impl<RadioDriver: Radio> embedded_io::ErrorType for BluetoothController<'_, RadioDriver> {
    type Error = Error;
}

impl<RadioDriver: Radio> Transport for BluetoothController<'_, RadioDriver> {
    async fn read<'buffer, Packet: PacketToHost<'buffer>>(
        &self,
        receive_buffer: &'buffer mut [u8],
    ) -> Result<Packet, Self::Error> {
        let mut staging = [0; MAXIMUM_HOST_PACKET_LENGTH];

        let (kind, length) = poll_fn(|context| {
            self.state.with_host_interface(|host_interface| {
                match host_interface.next_packet_for_host(&mut staging) {
                    Some(packet) => Poll::Ready(packet),
                    None => {
                        self.state.host_waker.register(context.waker());
                        Poll::Pending
                    }
                }
            })
        })
        .await;
        self.state.room_waker.wake();

        let mut packet_bytes = &staging[..length];
        Ok(Packet::read_hci(
            TransportPacketKind::from(kind),
            &mut packet_bytes,
            receive_buffer,
        )?)
    }

    async fn write<Packet: PacketToController>(&self, packet: &Packet) -> Result<(), Self::Error> {
        let mut bytes = [0; 1 + 3 + 255];
        let length =
            crate::host_interface::serialise_host_packet(packet, &mut bytes).ok_or(Error::MalformedPacket)?;
        let without_indicator = &bytes[1..length];

        match Packet::KIND {
            TransportPacketKind::Cmd => {
                poll_fn(|context| {
                    self.state.with_host_interface(|host_interface| {
                        if !host_interface.can_take_command() {
                            self.state.room_waker.register(context.waker());
                            return Poll::Pending;
                        }
                        let now = self.radio.now();
                        let result = self.radio.with_transmit_buffer(|transmit_buffer| {
                            host_interface.handle_command(without_indicator, now, transmit_buffer)
                        });
                        if let Ok(Some(operation)) = result {
                            self.radio.run(operation);
                        }
                        Poll::Ready(result.map(|_| ()))
                    })
                })
                .await?;
                self.state.host_waker.wake();
                Ok(())
            }

            TransportPacketKind::AclData => {
                poll_fn(|context| {
                    self.state.with_host_interface(|host_interface| {
                        match host_interface.handle_acl_data(without_indicator) {
                            Ok(Err(())) => {
                                self.state.room_waker.register(context.waker());
                                Poll::Pending
                            }
                            Ok(Ok(())) => Poll::Ready(Ok(())),
                            Err(error) => Poll::Ready(Err(Error::from(error))),
                        }
                    })
                })
                .await
            }

            _ => Err(Error::MalformedPacket),
        }
    }
}

const _: () = assert!(RADIO_BUFFER_LENGTH >= 2 + 255);

#[cfg(test)]
#[path = "controller_tests.rs"]
mod controller_tests;
