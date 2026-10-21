// SPDX-License-Identifier: GPL-3.0-or-later
//! Packet-level tests for the PTP/MTP codec and a scripted end-to-end session over a fake
//! bulk transport (no hardware).
use deckle_transport::ptp::{
    Container, ContainerKind, DeviceInfo, ObjectInfo, PtpError, decode_string, decode_u32_array,
    encode_string, op, rsp,
};
use deckle_transport::session::{Mtp, Transport, TransportError};

#[test]
fn command_container_round_trips() {
    let c = Container {
        kind: ContainerKind::Command,
        code: op::OPEN_SESSION,
        tx_id: 1,
        params: vec![1],
    };
    let bytes = c.encode(&[]);
    assert_eq!(
        bytes,
        [16, 0, 0, 0, 1, 0, 0x02, 0x10, 1, 0, 0, 0, 1, 0, 0, 0]
    );
    let (parsed, payload) = Container::decode(&bytes).unwrap();
    assert_eq!(parsed, c);
    assert!(payload.is_empty());
}

#[test]
fn data_container_carries_a_payload_and_reports_its_length() {
    let c = Container {
        kind: ContainerKind::Data,
        code: op::SEND_OBJECT,
        tx_id: 7,
        params: vec![],
    };
    let bytes = c.encode(b"hello");
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 12 + 5);
    let (parsed, payload) = Container::decode(&bytes).unwrap();
    assert_eq!(parsed.kind, ContainerKind::Data);
    assert_eq!(payload, b"hello");
    assert!(Container::decode(&bytes[..10]).is_err(), "truncated header");
}

#[test]
fn ptp_strings_are_utf16_with_a_count_byte_and_terminator() {
    let e = encode_string("ab");
    assert_eq!(e, [3, b'a', 0, b'b', 0, 0, 0]);
    assert_eq!(encode_string(""), [0]);
    let (s, used) = decode_string(&e).unwrap();
    assert_eq!(s, "ab");
    assert_eq!(used, 7);
    let (empty, used) = decode_string(&[0]).unwrap();
    assert_eq!(empty, "");
    assert_eq!(used, 1);
}

#[test]
fn u32_arrays_decode_with_their_count() {
    let bytes = [2, 0, 0, 0, 0x01, 0, 1, 0, 0x02, 0, 1, 0];
    assert_eq!(
        decode_u32_array(&bytes).unwrap(),
        vec![0x0001_0001, 0x0001_0002]
    );
}

#[test]
fn object_info_always_uses_the_32_bit_layout_and_round_trips() {
    // libmtp #436: Kindles reject the 64-bit ObjectCompressedSize variant; we never emit it.
    let info = ObjectInfo {
        storage_id: 0x0001_0001,
        format: 0x3000,
        size: 4242,
        parent: 0x11,
        association: false,
        filename: "x.kfx".into(),
    };
    let bytes = info.encode();
    assert_eq!(
        bytes.len(),
        52 + encode_string("x.kfx").len() + 3,
        "fixed fields + filename + three empty strings"
    );
    assert_eq!(&bytes[8..12], &4242u32.to_le_bytes());
    let parsed = ObjectInfo::decode(&bytes).unwrap();
    assert_eq!(parsed, info);
    // the Kindle's nameless root folders decode too (empty filename, then a date string)
    let mut nameless = ObjectInfo {
        storage_id: 1,
        format: 0x3001,
        size: 0,
        parent: 0,
        association: true,
        filename: String::new(),
    }
    .encode();
    let date_at = 52;
    nameless.splice(date_at + 1..date_at + 2, encode_string("20240101T000000"));
    let parsed = ObjectInfo::decode(&nameless).unwrap();
    assert!(parsed.association);
    assert_eq!(parsed.filename, "");
}

/// A scripted bulk transport: each `expect` is (command bytes we must receive, replies to return).
#[derive(Default)]
struct Fake {
    script: Vec<(Vec<u8>, Vec<Vec<u8>>)>,
    pos: usize,
    pending: Vec<Vec<u8>>,
    sent: Vec<Vec<u8>>,
}

impl Transport for Fake {
    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.sent.push(bytes.to_vec());
        if let Some((expected, replies)) = self.script.get(self.pos)
            && expected == bytes
        {
            self.pending = replies.clone();
            self.pos += 1;
        }
        Ok(())
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        if self.pending.is_empty() {
            eprintln!(
                "FAKE: no reply pending; script pos {} of {}; sent {:?}",
                self.pos,
                self.script.len(),
                self.sent.iter().map(|w| w.len()).collect::<Vec<_>>()
            );
            return Err(TransportError::Timeout);
        }
        let r = self.pending.remove(0);
        buf[..r.len()].copy_from_slice(&r);
        Ok(r.len())
    }
}

fn cmd(code: u16, tx: u32, params: &[u32]) -> Vec<u8> {
    Container {
        kind: ContainerKind::Command,
        code,
        tx_id: tx,
        params: params.to_vec(),
    }
    .encode(&[])
}
fn ok(code: u16, tx: u32, params: &[u32]) -> Vec<u8> {
    let _ = code;
    Container {
        kind: ContainerKind::Response,
        code: rsp::OK,
        tx_id: tx,
        params: params.to_vec(),
    }
    .encode(&[])
}
fn data(code: u16, tx: u32, payload: &[u8]) -> Vec<u8> {
    Container {
        kind: ContainerKind::Data,
        code,
        tx_id: tx,
        params: vec![],
    }
    .encode(payload)
}

#[test]
fn a_scripted_session_finds_documents_and_sends_a_file() {
    let storage = 0x0001_0001u32;
    let docs_info = ObjectInfo {
        storage_id: storage,
        format: 0x3001,
        size: 0,
        parent: 0,
        association: true,
        filename: "documents".into(),
    }
    .encode();
    let fake = Fake {
        script: vec![
            (
                cmd(op::OPEN_SESSION, 0, &[1]),
                vec![ok(op::OPEN_SESSION, 0, &[])],
            ),
            (
                cmd(op::GET_STORAGE_IDS, 1, &[]),
                vec![
                    data(op::GET_STORAGE_IDS, 1, &[1, 0, 0, 0, 1, 0, 1, 0]),
                    ok(op::GET_STORAGE_IDS, 1, &[]),
                ],
            ),
            (
                cmd(op::GET_OBJECT_HANDLES, 2, &[storage, 0, 0xFFFF_FFFF]),
                vec![
                    data(op::GET_OBJECT_HANDLES, 2, &[1, 0, 0, 0, 0x2A, 0, 0, 0]),
                    ok(op::GET_OBJECT_HANDLES, 2, &[]),
                ],
            ),
            (
                cmd(op::GET_OBJECT_INFO, 3, &[0x2A]),
                vec![
                    data(op::GET_OBJECT_INFO, 3, &docs_info),
                    ok(op::GET_OBJECT_INFO, 3, &[]),
                ],
            ),
            (cmd(op::SEND_OBJECT_INFO, 4, &[storage, 0x2A]), vec![]),
        ],
        ..Default::default()
    };
    let mut mtp = Mtp::new(Box::new(fake));
    mtp.open_session().unwrap();
    assert_eq!(mtp.storages().unwrap(), vec![storage]);
    let docs = mtp
        .find_child(storage, 0, "documents")
        .unwrap()
        .expect("documents folder");
    assert_eq!(docs, 0x2A);
    // SendObjectInfo: command, then our ObjectInfo as a data phase, then the response with the
    // new handle; then SendObject with the bytes.
    let fake = mtp.transport_mut::<Fake>().unwrap();
    let expected_info = ObjectInfo {
        storage_id: storage,
        format: 0x3000,
        size: 5,
        parent: 0x2A,
        association: false,
        filename: "b.kfx".into(),
    }
    .encode();
    fake.script.push((
        data(op::SEND_OBJECT_INFO, 4, &expected_info),
        vec![ok(op::SEND_OBJECT_INFO, 4, &[storage, 0x2A, 0x99])],
    ));
    fake.script.push((cmd(op::SEND_OBJECT, 5, &[]), vec![]));
    fake.script.push((
        data(op::SEND_OBJECT, 5, b"hello"),
        vec![ok(op::SEND_OBJECT, 5, &[])],
    ));
    let handle = mtp.send_file(storage, docs, "b.kfx", b"hello").unwrap();
    assert_eq!(handle, 0x99);
}

#[test]
fn an_error_response_is_surfaced_with_its_code() {
    let fake = Fake {
        script: vec![(
            cmd(op::OPEN_SESSION, 0, &[1]),
            vec![
                Container {
                    kind: ContainerKind::Response,
                    code: 0x2019,
                    tx_id: 0,
                    params: vec![],
                }
                .encode(&[]),
            ],
        )],
        ..Default::default()
    };
    let mut mtp = Mtp::new(Box::new(fake));
    let err = mtp.open_session().unwrap_err();
    assert!(err.to_string().contains("0x2019"), "{err}");
}

#[test]
fn a_data_phase_that_fills_whole_packets_gets_a_zero_length_packet_both_ways() {
    // 512 - 12 header bytes = 500 payload bytes -> exactly one wMaxPacketSize on the wire.
    let payload = vec![7u8; 500];
    let storage = 0x0001_0001u32;
    let expected_info = ObjectInfo {
        storage_id: storage,
        format: 0x3000,
        size: 500,
        parent: 0x2A,
        association: false,
        filename: "z.kfx".into(),
    }
    .encode();
    let fake = Fake {
        script: vec![
            (
                cmd(op::OPEN_SESSION, 0, &[1]),
                vec![ok(op::OPEN_SESSION, 0, &[])],
            ),
            (cmd(op::SEND_OBJECT_INFO, 1, &[storage, 0x2A]), vec![]),
            (
                data(op::SEND_OBJECT_INFO, 1, &expected_info),
                vec![ok(op::SEND_OBJECT_INFO, 1, &[storage, 0x2A, 0x77])],
            ),
            (cmd(op::SEND_OBJECT, 2, &[]), vec![]),
            (data(op::SEND_OBJECT, 2, &payload), vec![]),
            // the host's zero-length packet, then the device answers with its own ZLP first
            (vec![], vec![vec![], ok(op::SEND_OBJECT, 2, &[])]),
        ],
        ..Default::default()
    };
    let mut mtp = Mtp::new(Box::new(fake));
    mtp.open_session().unwrap();
    let h = mtp.send_file(storage, 0x2A, "z.kfx", &payload).unwrap();
    assert_eq!(h, 0x77);
    let fake = mtp.transport_mut::<Fake>().unwrap();
    assert!(
        fake.sent.iter().any(|w| w.is_empty()),
        "a zero-length packet must be written"
    );
}

#[test]
fn documents_folder_is_found_under_a_nameless_root_association_and_never_invented() {
    let storage = 0x0001_0001u32;
    let nameless = ObjectInfo {
        storage_id: storage,
        format: 0x3001,
        size: 0,
        parent: 0,
        association: true,
        filename: String::new(),
    }
    .encode();
    let docs_info = ObjectInfo {
        storage_id: storage,
        format: 0x3001,
        size: 0,
        parent: 0x10,
        association: true,
        filename: "documents".into(),
    }
    .encode();
    let fake = Fake {
        script: vec![
            (
                cmd(op::OPEN_SESSION, 0, &[1]),
                vec![ok(op::OPEN_SESSION, 0, &[])],
            ),
            (
                cmd(op::GET_OBJECT_HANDLES, 1, &[storage, 0, 0xFFFF_FFFF]),
                vec![
                    data(op::GET_OBJECT_HANDLES, 1, &[1, 0, 0, 0, 0x10, 0, 0, 0]),
                    ok(op::GET_OBJECT_HANDLES, 1, &[]),
                ],
            ),
            (
                cmd(op::GET_OBJECT_INFO, 2, &[0x10]),
                vec![
                    data(op::GET_OBJECT_INFO, 2, &nameless),
                    ok(op::GET_OBJECT_INFO, 2, &[]),
                ],
            ),
            (
                cmd(op::GET_OBJECT_HANDLES, 3, &[storage, 0, 0x10]),
                vec![
                    data(op::GET_OBJECT_HANDLES, 3, &[1, 0, 0, 0, 0x11, 0, 0, 0]),
                    ok(op::GET_OBJECT_HANDLES, 3, &[]),
                ],
            ),
            (
                cmd(op::GET_OBJECT_INFO, 4, &[0x11]),
                vec![
                    data(op::GET_OBJECT_INFO, 4, &docs_info),
                    ok(op::GET_OBJECT_INFO, 4, &[]),
                ],
            ),
            // an empty storage: no documents anywhere -> error, nothing created
            (
                cmd(op::GET_OBJECT_HANDLES, 5, &[storage, 0, 0xFFFF_FFFF]),
                vec![
                    data(op::GET_OBJECT_HANDLES, 5, &[0, 0, 0, 0]),
                    ok(op::GET_OBJECT_HANDLES, 5, &[]),
                ],
            ),
        ],
        ..Default::default()
    };
    let mut mtp = Mtp::new(Box::new(fake));
    mtp.open_session().unwrap();
    assert_eq!(mtp.find_documents(storage).unwrap(), 0x11);
    let err = mtp.find_documents(storage).unwrap_err();
    assert!(err.to_string().contains("no documents folder"), "{err}");
    let fake = mtp.transport_mut::<Fake>().unwrap();
    assert!(
        !fake
            .sent
            .iter()
            .any(|w| w.len() >= 8 && u16::from_le_bytes([w[6], w[7]]) == op::SEND_OBJECT_INFO),
        "must not create folders"
    );
}

#[test]
fn get_object_downloads_a_multi_chunk_file() {
    let payload: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
    let mut whole = data(op::GET_OBJECT, 1, &payload);
    // the device splits the data phase across USB transfers
    let tail = whole.split_off(1000);
    let fake = Fake {
        script: vec![
            (
                cmd(op::OPEN_SESSION, 0, &[1]),
                vec![ok(op::OPEN_SESSION, 0, &[])],
            ),
            (
                cmd(op::GET_OBJECT, 1, &[0x42]),
                vec![whole, tail, ok(op::GET_OBJECT, 1, &[])],
            ),
        ],
        ..Default::default()
    };
    let mut mtp = Mtp::new(Box::new(fake));
    mtp.open_session().unwrap();
    assert_eq!(mtp.get_object(0x42).unwrap(), payload);
}

fn sample_device_info() -> DeviceInfo {
    DeviceInfo {
        manufacturer: "Amazon".into(),
        model: "Kindle Scribe".into(),
        device_version: "5.18.1.0.1".into(),
        serial_number: "G0X1".into(),
    }
}

#[test]
fn device_info_round_trips_past_the_operation_arrays() {
    let bytes = sample_device_info().encode();
    assert_eq!(DeviceInfo::decode(&bytes).unwrap(), sample_device_info());
    assert_eq!(
        DeviceInfo::decode(&bytes[..bytes.len() - 3]),
        Err(PtpError::BadString)
    );
    assert_eq!(DeviceInfo::decode(&bytes[..10]), Err(PtpError::Truncated));
}

#[test]
fn device_info_reads_the_firmware_inside_a_session() {
    let fake = Fake {
        script: vec![
            (
                cmd(op::OPEN_SESSION, 0, &[1]),
                vec![ok(op::OPEN_SESSION, 0, &[])],
            ),
            (
                cmd(op::GET_DEVICE_INFO, 1, &[]),
                vec![
                    data(op::GET_DEVICE_INFO, 1, &sample_device_info().encode()),
                    ok(op::GET_DEVICE_INFO, 1, &[]),
                ],
            ),
        ],
        ..Default::default()
    };
    let mut mtp = Mtp::new(Box::new(fake));
    mtp.open_session().unwrap();
    let info = mtp.device_info().unwrap();
    assert_eq!(info.device_version, "5.18.1.0.1");
    assert_eq!(info.model, "Kindle Scribe");
}

#[test]
fn storage_info_decodes_capacity_and_free_space() {
    let info = deckle_transport::ptp::StorageInfo {
        max_capacity: 31_000_000_000,
        free_bytes: 12_345_678_901,
        description: "Internal Storage".into(),
        volume_label: "Kindle".into(),
    };
    let bytes = info.encode();
    assert_eq!(
        deckle_transport::ptp::StorageInfo::decode(&bytes).unwrap(),
        info
    );
    // Some devices stop after the description.
    let short = &bytes[..bytes.len() - (1 + 2 * 7)];
    let got = deckle_transport::ptp::StorageInfo::decode(short).unwrap();
    assert_eq!(got.free_bytes, 12_345_678_901);
    assert_eq!(got.volume_label, "");
    assert!(deckle_transport::ptp::StorageInfo::decode(&bytes[..20]).is_err());
}
