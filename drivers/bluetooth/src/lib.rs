//! A Bluetooth LE controller for the PineTime's nRF52832 radio, written in Rust, for the TrouBLE host.
//!
//! It is a peripheral with one connection on the 1 megabit PHY, without encryption yet. TrouBLE
//! drives it through `bt_hci::controller::ExternalController`, as it would a controller on a
//! separate chip. Everything but `nrf_radio` is plain logic, so it runs in tests on a computer.

#![no_std]

mod controller;
mod host_interface;
mod link_layer;
mod packets;

#[cfg(target_os = "none")]
mod nrf_radio;

pub use controller::{BluetoothController, ControllerState, Error, Radio, RadioEvent};
pub use host_interface::HostInterface;
pub use link_layer::RadioOperation;
pub use packets::Channel;

#[cfg(target_os = "none")]
pub use nrf_radio::{start, NrfRadio, RadioInterruptHandler, RadioPeripherals};

#[cfg(test)]
mod tests;
