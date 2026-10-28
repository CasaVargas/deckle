// SPDX-License-Identifier: GPL-3.0-or-later
//! The USB serial number behind a mounted Kindle volume: what tells two Kindles of one model
//! apart. A mass-storage Kindle has no serial on its disk, only in its USB descriptor, so it's
//! asked of the operating system. Any failure is `None`, and the Kindle is then known by its
//! model and volume name alone, as before.

use std::path::Path;

/// The USB serial of the device `root` (a mounted volume) is on, when the OS will say.
pub fn serial_for_mount(root: &Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let plist = run("diskutil", &["info", "-plist", &root.to_string_lossy()])?;
        let disk = parent_disk(&plist)?;
        let ioreg = run("ioreg", &["-r", "-c", "IOUSBHostDevice", "-l", "-w0"])?;
        ioreg_serial(&ioreg, &disk)
    }
    #[cfg(target_os = "windows")]
    {
        let script = "Get-CimInstance Win32_DiskDrive | Where-Object { $_.InterfaceType -eq 'USB' } | \
            ForEach-Object { $d = $_; \
            Get-CimInstance -Query \"ASSOCIATORS OF {Win32_DiskDrive.DeviceID='$($d.DeviceID.Replace('\\','\\\\'))'} WHERE AssocClass=Win32_DiskDriveToDiskPartition\" | \
            ForEach-Object { Get-CimInstance -Query \"ASSOCIATORS OF {Win32_DiskPartition.DeviceID='$($_.DeviceID)'} WHERE AssocClass=Win32_LogicalDiskToPartition\" | \
            ForEach-Object { \"$($_.DeviceID)|$($d.PNPDeviceID)\" } } }";
        let out = run(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", script],
        )?;
        windows_serial(&out, &root.to_string_lossy())
    }
    #[cfg(target_os = "linux")]
    {
        let source = run("findmnt", &["-no", "SOURCE", &root.to_string_lossy()])?;
        let props = run(
            "udevadm",
            &[
                "info",
                "--query=property",
                &format!("--name={}", source.trim()),
            ],
        )?;
        udev_serial(&props)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = root;
        None
    }
}

#[allow(dead_code)]
fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = crate::quiet::command(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `ParentWholeDisk` (`disk21`) from `diskutil info -plist`.
#[allow(dead_code)]
fn parent_disk(plist: &str) -> Option<String> {
    let after = plist.split("<key>ParentWholeDisk</key>").nth(1)?;
    let value = after.split("<string>").nth(1)?.split("</string>").next()?;
    Some(value.trim().to_owned()).filter(|v| !v.is_empty())
}

/// The serial of the USB device whose subtree holds the disk `bsd` (`disk21`), from
/// `ioreg -r -c IOUSBHostDevice -l -w0`. The nearest USB device above the disk is the Kindle
/// itself, not the hub it hangs off.
#[allow(dead_code)]
fn ioreg_serial(ioreg: &str, bsd: &str) -> Option<String> {
    let lines: Vec<&str> = ioreg.lines().collect();
    // Every tree node: (line, column of its `+-o`).
    let nodes: Vec<(usize, usize)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.find("+-o ").map(|c| (i, c)))
        .collect();
    let want = format!("\"BSD Name\" = \"{bsd}\"");
    let hit = lines.iter().position(|l| l.contains(&want))?;
    let mut best: Option<usize> = None;
    for (k, &(i, col)) in nodes.iter().enumerate() {
        if !lines[i].contains("class IOUSBHostDevice") {
            continue;
        }
        let end = nodes[k + 1..]
            .iter()
            .find(|&&(_, c)| c <= col)
            .map_or(lines.len(), |&(j, _)| j);
        if i < hit && hit < end && best.is_none_or(|b| k > b) {
            best = Some(k);
        }
    }
    let k = best?;
    // The node's own properties come before its first child.
    let own_end = nodes.get(k + 1).map_or(lines.len(), |&(j, _)| j);
    lines[nodes[k].0..own_end].iter().find_map(|l| {
        let value = l.split_once("\"kUSBSerialNumberString\" = \"")?.1;
        let value = value.split('"').next()?.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

/// The serial of the drive `drive` (`E:` or `E:\`) from lines of `<drive>|<PNP device id>`. A
/// USB storage id ends `<serial>&0`; one Windows made up (`8&2f3a1b2&0`) has no real serial.
#[allow(dead_code)]
fn windows_serial(out: &str, drive: &str) -> Option<String> {
    let want = drive.get(..2)?.to_ascii_uppercase();
    out.lines().find_map(|l| {
        let (d, pnp) = l.trim().split_once('|')?;
        if d.to_ascii_uppercase() != want {
            return None;
        }
        let instance = pnp.rsplit('\\').next()?;
        let serial = instance.split('&').next()?;
        (serial.len() >= 6).then(|| serial.to_owned())
    })
}

/// `ID_SERIAL_SHORT` from `udevadm info --query=property`.
#[allow(dead_code)]
fn udev_serial(props: &str) -> Option<String> {
    props
        .lines()
        .find_map(|l| l.strip_prefix("ID_SERIAL_SHORT="))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOREG: &str = "\
+-o Root  <class IORegistryEntry>
  +-o USB2 Hub@02100000  <class IOUSBHostDevice, id 0x1, registered>
    | {
    |   \"kUSBSerialNumberString\" = \"HUBSERIAL\"
    | }
    |
    +-o Internal Storage@02120000  <class IOUSBHostDevice, id 0x2, registered>
      | {
      |   \"kUSBSerialNumberString\" = \"G0B0AAAA11112222\"
      | }
      |
      +-o IOUSBHostInterface@0  <class IOUSBHostInterface, id 0x3>
        +-o IOMedia  <class IOMedia, id 0x4>
          | {
          |   \"BSD Name\" = \"disk21\"
          | }
    +-o Other Stick@02130000  <class IOUSBHostDevice, id 0x5, registered>
      | {
      |   \"kUSBSerialNumberString\" = \"STICK9999\"
      | }
      +-o IOMedia  <class IOMedia, id 0x6>
          | {
          |   \"BSD Name\" = \"disk22\"
          | }
";

    #[test]
    fn the_device_that_owns_the_disk_gives_its_serial_not_its_hubs() {
        assert_eq!(
            ioreg_serial(IOREG, "disk21").as_deref(),
            Some("G0B0AAAA11112222")
        );
        assert_eq!(ioreg_serial(IOREG, "disk22").as_deref(), Some("STICK9999"));
        assert_eq!(ioreg_serial(IOREG, "disk99"), None);
    }

    #[test]
    fn the_whole_disk_comes_from_diskutil() {
        let plist = "<plist><dict>\n\t<key>BusProtocol</key>\n\t<string>USB</string>\n\
                     \t<key>ParentWholeDisk</key>\n\t<string>disk21</string>\n</dict></plist>";
        assert_eq!(parent_disk(plist).as_deref(), Some("disk21"));
        assert_eq!(parent_disk("<plist></plist>"), None);
    }

    #[test]
    fn windows_pairs_a_drive_with_its_usb_instance() {
        let out = "C:|SCSI\\DISK&VEN_NVME\\4&1\r\nE:|USBSTOR\\DISK&VEN_AMAZON&PROD_KINDLE_INTERNAL_STORAGE\\G0B0AAAA11112222&0\r\n";
        assert_eq!(
            windows_serial(out, "E:\\").as_deref(),
            Some("G0B0AAAA11112222")
        );
        assert_eq!(
            windows_serial(out, "e:").as_deref(),
            Some("G0B0AAAA11112222")
        );
        assert_eq!(windows_serial(out, "F:"), None);
        // A generated instance id (no real serial) says nothing.
        assert_eq!(
            windows_serial("E:|USBSTOR\\DISK&VEN_X\\8&2F3A&0\r\n", "E:"),
            None
        );
    }

    #[test]
    fn linux_reads_the_short_serial_from_udev() {
        let props = "ID_BUS=usb\nID_SERIAL=Amazon_Kindle_G0B0AAAA11112222\nID_SERIAL_SHORT=G0B0AAAA11112222\n";
        assert_eq!(udev_serial(props).as_deref(), Some("G0B0AAAA11112222"));
        assert_eq!(udev_serial("ID_BUS=usb\n"), None);
    }
}
