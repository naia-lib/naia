//! Decoder behaviour on hostile input.
//!
//! Contract under test:
//!
//! > A decoder fed bytes chosen by a remote peer either returns a value or
//! > returns `SerdeErr`. It never panics, and it never allocates more than the
//! > input it was given could plausibly fill.
//!
//! Both properties matter at the wire boundary: naia decodes packets straight
//! from unauthenticated peers, and the established handling of a `SerdeErr` is
//! to warn and drop the packet. A panic or an out-of-memory abort escapes that
//! handling entirely -- an abort is not even catchable as an `Err`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Records the largest single allocation any decoder in this binary asked for.
///
/// The eager-allocation bug is invisible to a plain `is_err()` assertion: with
/// overcommit the runtime happily reserves the address space and the decode
/// still ends in `Err` when the reader runs dry. What actually has to be pinned
/// is the *request*, so the allocator itself is the observer.
struct PeakTracking;

static PEAK_ALLOC: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for PeakTracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        PEAK_ALLOC.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        PEAK_ALLOC.fetch_max(new_size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: PeakTracking = PeakTracking;

/// Nothing in this file legitimately needs a megabyte.
const PEAK_LIMIT: usize = 1024 * 1024;

use naia_serde::{BitReader, BitWrite, BitWriter, Serde, UnsignedVariableInteger};

/// A buffer whose only content is a length prefix claiming a huge number of
/// elements follow. Before the pre-allocation was bounded, decoding this asked
/// the allocator for tens of gigabytes and aborted the process.
fn forged_length_prefix<const BITS: u8>(claimed: u64) -> Vec<u8> {
    let mut writer = BitWriter::new();
    UnsignedVariableInteger::<BITS>::new(claimed).ser(&mut writer);
    writer.to_bytes().to_vec()
}

/// The four length-prefixed decoders, each fed a few bytes claiming four
/// billion elements follow. Every one must refuse, and none may size its buffer
/// from the claim rather than from the input actually in hand.
#[test]
fn forged_lengths_do_not_preallocate() {
    PEAK_ALLOC.store(0, Ordering::Relaxed);

    let vec_prefix = forged_length_prefix::<5>(u32::MAX as u64);
    let byte_prefix = forged_length_prefix::<9>(u32::MAX as u64);

    assert!(Vec::<i32>::de(&mut BitReader::new(&vec_prefix)).is_err());
    assert!(VecDeque::<i32>::de(&mut BitReader::new(&vec_prefix)).is_err());
    assert!(String::de(&mut BitReader::new(&byte_prefix)).is_err());
    assert!(Box::<[u8]>::de(&mut BitReader::new(&byte_prefix)).is_err());

    let peak = PEAK_ALLOC.load(Ordering::Relaxed);
    assert!(
        peak < PEAK_LIMIT,
        "a {}-byte packet caused a {peak}-byte allocation",
        vec_prefix.len(),
    );
}

/// A truncated but honest-looking length still has to decode the elements it
/// promised, so a short read remains an error rather than a short `Vec`.
#[test]
fn truncated_vec_body_is_an_error() {
    let mut writer = BitWriter::new();
    vec![1i32, 2, 3].ser(&mut writer);
    let bytes = writer.to_bytes();
    let truncated = &bytes[..bytes.len() - 2];
    let mut reader = BitReader::new(truncated);
    assert!(Vec::<i32>::de(&mut reader).is_err());
}

/// The continuation bit of a variable-length integer is wire-controlled, so a
/// peer can keep the decode loop running past the width of the accumulator.
/// This used to shift past 128 bits: a panic in debug, silent corruption in
/// release.
#[test]
fn overlong_variable_integer_errors_instead_of_overflowing() {
    let mut writer = BitWriter::new();
    // 64 chunks of 5 value bits: 320 bits of claimed value, far past u128.
    for _ in 0..64 {
        writer.write_bit(true); // continue
        for _ in 0..5 {
            writer.write_bit(true);
        }
    }
    writer.write_bit(false); // terminate
    for _ in 0..5 {
        writer.write_bit(false);
    }
    let buffer = writer.to_bytes();

    let mut reader = BitReader::new(&buffer);
    assert!(UnsignedVariableInteger::<5>::de(&mut reader).is_err());
}

/// The bound must not reject values that legitimately reach the top of the
/// accumulator -- including the chunk that straddles bit 128.
#[test]
fn maximal_variable_integer_still_round_trips() {
    for value in [0u64, 1, u32::MAX as u64, u64::MAX] {
        let mut writer = BitWriter::new();
        UnsignedVariableInteger::<5>::new(value).ser(&mut writer);
        let buffer = writer.to_bytes();
        let mut reader = BitReader::new(&buffer);
        let out = UnsignedVariableInteger::<5>::de(&mut reader).unwrap();
        assert_eq!(out.get(), value as i128);
    }
}
