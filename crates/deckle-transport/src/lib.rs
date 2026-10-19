// SPDX-License-Identifier: GPL-3.0-or-later
//! MTP/PTP client for MTP-only Kindles. `ptp` is the wire codec, `session` the transaction
//! logic over a pluggable bulk transport, `usb` the libusb backend.

pub mod ptp;
pub mod session;
pub mod usb;
