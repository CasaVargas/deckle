// SPDX-License-Identifier: GPL-3.0-or-later
//! A PalmDB file in memory: its records by index.

use crate::MobiError;

pub struct PalmDb<'a> {
    data: &'a [u8],
    offsets: Vec<usize>,
}

impl<'a> PalmDb<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, MobiError> {
        if data.len() < 78 || !crate::is_mobi(data) {
            return Err(MobiError::NotMobi("no BOOKMOBI type and creator"));
        }
        let count = usize::from(u16::from_be_bytes([data[76], data[77]]));
        let list = data
            .get(78..78 + count * 8)
            .ok_or(MobiError::NotMobi("record list cut short"))?;
        let mut offsets: Vec<usize> = (0..count)
            .map(|i| u32::from_be_bytes(list[i * 8..i * 8 + 4].try_into().unwrap()) as usize)
            .collect();
        if offsets.windows(2).any(|w| w[1] < w[0])
            || offsets.last().is_some_and(|o| *o > data.len())
        {
            return Err(MobiError::NotMobi("record offsets out of order"));
        }
        offsets.push(data.len());
        Ok(PalmDb { data, offsets })
    }

    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Record `i`; empty past the end.
    pub fn record(&self, i: usize) -> &'a [u8] {
        match (self.offsets.get(i), self.offsets.get(i + 1)) {
            (Some(&a), Some(&b)) => &self.data[a..b],
            _ => &[],
        }
    }
}

pub fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

pub fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
