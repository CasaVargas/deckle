// SPDX-License-Identifier: GPL-3.0-or-later
//! libusb backend: find Kindles (Amazon vendor id 0x1949) and open a bulk pipe to their MTP
//! interface. Only one program may hold the session; close Android File Transfer, Send to
//! Kindle and calibre first.

use std::time::Duration;

use rusb::{Context, Device, DeviceHandle, UsbContext};
use serde::Serialize;

use crate::session::{Mtp, Transport, TransportError};

pub const AMAZON_VENDOR_ID: u16 = 0x1949;
/// libmtp uses LONG_TIMEOUT (60 s) for Kindles: SendObject answers only after the file is on eMMC.
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MtpInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: Option<String>,
    pub product: Option<String>,
    pub bus: u8,
    pub address: u8,
}

/// Kindles exposing an MTP (still-image class 6) interface.
pub fn detect() -> Vec<MtpInfo> {
    let Ok(ctx) = Context::new() else {
        return Vec::new();
    };
    let Ok(devices) = ctx.devices() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for d in devices.iter() {
        let Ok(desc) = d.device_descriptor() else {
            continue;
        };
        if desc.vendor_id() != AMAZON_VENDOR_ID {
            continue;
        }
        if find_mtp_interface(&d).is_none() {
            continue;
        }
        let (serial, product) = match d.open() {
            Ok(h) => (
                h.read_serial_number_string_ascii(&desc).ok(),
                h.read_product_string_ascii(&desc).ok(),
            ),
            Err(_) => (None, None),
        };
        out.push(MtpInfo {
            vendor_id: desc.vendor_id(),
            product_id: desc.product_id(),
            serial,
            product,
            bus: d.bus_number(),
            address: d.address(),
        });
    }
    out
}

struct Endpoints {
    interface: u8,
    bulk_in: u8,
    bulk_out: u8,
    max_packet_out: usize,
}

fn find_mtp_interface<T: UsbContext>(d: &Device<T>) -> Option<Endpoints> {
    let config = d.active_config_descriptor().ok()?;
    for iface in config.interfaces() {
        for desc in iface.descriptors() {
            let is_ptp = desc.class_code() == 6 && desc.sub_class_code() == 1;
            let is_vendor_mtp = desc.class_code() == 0xFF && desc.sub_class_code() == 0xFF;
            if !(is_ptp || is_vendor_mtp) {
                continue;
            }
            let (mut bulk_in, mut bulk_out, mut max_out) = (None, None, 512usize);
            for ep in desc.endpoint_descriptors() {
                if ep.transfer_type() == rusb::TransferType::Bulk {
                    match ep.direction() {
                        rusb::Direction::In => bulk_in = Some(ep.address()),
                        rusb::Direction::Out => {
                            bulk_out = Some(ep.address());
                            max_out = usize::from(ep.max_packet_size()).max(1);
                        }
                    }
                }
            }
            if let (Some(i), Some(o)) = (bulk_in, bulk_out) {
                return Some(Endpoints {
                    interface: desc.interface_number(),
                    bulk_in: i,
                    bulk_out: o,
                    max_packet_out: max_out,
                });
            }
        }
    }
    None
}

pub struct UsbTransport {
    handle: DeviceHandle<Context>,
    ep: Endpoints,
}

impl Transport for UsbTransport {
    fn max_packet_out(&self) -> usize {
        self.ep.max_packet_out
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        if bytes.is_empty() {
            // zero-length packet terminating a data phase
            self.handle
                .write_bulk(self.ep.bulk_out, &[], TIMEOUT)
                .map_err(map_err)?;
            return Ok(());
        }
        let mut off = 0;
        while off < bytes.len() {
            let n = self
                .handle
                .write_bulk(self.ep.bulk_out, &bytes[off..], TIMEOUT)
                .map_err(map_err)?;
            off += n;
        }
        Ok(())
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        self.handle
            .read_bulk(self.ep.bulk_in, buf, TIMEOUT)
            .map_err(map_err)
    }
}

fn map_err(e: rusb::Error) -> TransportError {
    match e {
        rusb::Error::Timeout => TransportError::Timeout,
        rusb::Error::Access | rusb::Error::Busy => TransportError::Usb(
            "another app holds the Kindle (Android File Transfer, Send to Kindle, Image Capture, calibre); quit it and retry".into(),
        ),
        other => TransportError::Usb(other.to_string()),
    }
}

impl Drop for UsbTransport {
    fn drop(&mut self) {
        // No USB reset, despite libmtp's FORCE_RESET_ON_CLOSE for Kindles: on a Scribe 2024 the
        // reset sends it to the home screen and off the bus until replugged, while a plain
        // close leaves it ready for the next session.
        let _ = self.handle.release_interface(self.ep.interface);
    }
}

/// Opens the device and claims its MTP interface (no session yet). Falls back to the serial
/// when the bus address is stale: a Kindle re-enumerates at a new one after a reset or replug.
pub fn open(info: &MtpInfo) -> Result<Mtp, TransportError> {
    let ctx = Context::new().map_err(map_err)?;
    let devices = ctx.devices().map_err(map_err)?;
    let at_address = devices
        .iter()
        .find(|d| d.bus_number() == info.bus && d.address() == info.address);
    let by_serial = || {
        let serial = info.serial.as_deref()?;
        devices.iter().find(|d| {
            let Ok(desc) = d.device_descriptor() else {
                return false;
            };
            desc.vendor_id() == AMAZON_VENDOR_ID
                && d.open()
                    .and_then(|h| h.read_serial_number_string_ascii(&desc))
                    .is_ok_and(|s| s == serial)
        })
    };
    if let Some(d) = at_address.or_else(by_serial) {
        let ep =
            find_mtp_interface(&d).ok_or_else(|| TransportError::Usb("no MTP interface".into()))?;
        let handle = d.open().map_err(map_err)?;
        let _ = handle.set_auto_detach_kernel_driver(true);
        handle.claim_interface(ep.interface).map_err(map_err)?;
        return Ok(Mtp::new(Box::new(UsbTransport { handle, ep })));
    }
    Err(TransportError::Usb("device is no longer connected".into()))
}
