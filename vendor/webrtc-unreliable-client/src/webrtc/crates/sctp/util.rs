use bytes::Bytes;
use crc::{Crc, CRC_32_ISCSI};

const PADDING_MULTIPLE: usize = 4;

pub(crate) fn get_padding_size(len: usize) -> usize {
    (PADDING_MULTIPLE - (len % PADDING_MULTIPLE)) % PADDING_MULTIPLE
}

/// Allocate and zero this data once.
/// We need to use it for the checksum and don't want to allocate/clear each time.
pub(crate) static FOUR_ZEROES: Bytes = Bytes::from_static(&[0, 0, 0, 0]);

/// Fastest way to do a crc32 without allocating.
pub(crate) fn generate_packet_checksum(raw: &Bytes) -> u32 {
    let hasher = Crc::<u32>::new(&CRC_32_ISCSI);
    let mut digest = hasher.digest();
    digest.update(&raw[0..8]);
    digest.update(&FOUR_ZEROES[..]);
    digest.update(&raw[12..]);
    digest.finalize()
}

/// Serial Number Arithmetic (RFC 1982)
#[inline]
pub(crate) fn sna32lt(i1: u32, i2: u32) -> bool {
    (i1 < i2 && i2 - i1 < 1 << 31) || (i1 > i2 && i1 - i2 > 1 << 31)
}

#[inline]
pub(crate) fn sna32lte(i1: u32, i2: u32) -> bool {
    i1 == i2 || sna32lt(i1, i2)
}

#[inline]
pub(crate) fn sna32gt(i1: u32, i2: u32) -> bool {
    (i1 < i2 && (i2 - i1) >= 1 << 31) || (i1 > i2 && (i1 - i2) <= 1 << 31)
}

#[inline]
pub(crate) fn sna32gte(i1: u32, i2: u32) -> bool {
    i1 == i2 || sna32gt(i1, i2)
}

#[inline]
pub(crate) fn sna16lt(i1: u16, i2: u16) -> bool {
    (i1 < i2 && (i2 - i1) < 1 << 15) || (i1 > i2 && (i1 - i2) > 1 << 15)
}

#[inline]
pub(crate) fn sna16lte(i1: u16, i2: u16) -> bool {
    i1 == i2 || sna16lt(i1, i2)
}

#[inline]
pub(crate) fn sna16gt(i1: u16, i2: u16) -> bool {
    (i1 < i2 && (i2 - i1) >= 1 << 15) || (i1 > i2 && (i1 - i2) <= 1 << 15)
}
