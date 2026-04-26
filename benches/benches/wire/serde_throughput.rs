use std::time::Duration;

use criterion::{criterion_group, BenchmarkId, Criterion, Throughput};

use naia_shared::{BitReader, BitWrite, BitWriter, Serde, UnsignedInteger, UnsignedVariableInteger};

/// Integers per bench iteration — chosen so the serialized output fits in the
/// 430-byte MTU buffer (worst case: fixed u16 × 128 = 256 bytes).
const N: usize = 128;

/// Serialize N fixed-width 16-bit unsigned integers into a BitWriter,
/// measuring encode throughput in bytes/sec.
fn encode_fixed_u16(c: &mut Criterion) {
    let values: Vec<u16> = (0..N as u16).collect();
    let bytes_per_iter = N * 2; // 16 bits = 2 bytes per value

    let mut group = c.benchmark_group("serde/encode");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(bytes_per_iter as u64));

    group.bench_function("fixed_u16", |b| {
        b.iter(|| {
            let mut writer = BitWriter::with_max_capacity();
            for &v in &values {
                UnsignedInteger::<16>::new(v).ser(&mut writer);
            }
            writer.to_bytes()
        })
    });
    group.finish();
}

/// Deserialize N fixed-width 16-bit unsigned integers from a BitReader.
fn decode_fixed_u16(c: &mut Criterion) {
    let values: Vec<u16> = (0..N as u16).collect();
    let bytes_per_iter = N * 2;

    // Pre-encode once; bench only the decode path.
    let encoded: Box<[u8]> = {
        let mut w = BitWriter::with_max_capacity();
        for &v in &values {
            UnsignedInteger::<16>::new(v).ser(&mut w);
        }
        w.to_bytes()
    };

    let mut group = c.benchmark_group("serde/decode");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(bytes_per_iter as u64));

    group.bench_function("fixed_u16", |b| {
        b.iter(|| {
            let mut reader = BitReader::new(&encoded);
            let mut sum = 0u64;
            for _ in 0..N {
                let v = UnsignedInteger::<16>::de(&mut reader).unwrap();
                sum = sum.wrapping_add(v.get() as u64);
            }
            sum
        })
    });
    group.finish();
}

/// Variable-length 7-bit chunks, values 0–127 (single-chunk, common case).
fn encode_varint7_small(c: &mut Criterion) {
    let values: Vec<u8> = (0..N as u8).collect();
    // Each value 0–127 encodes as 1 chunk of 7+1 = 8 bits → 1 byte.
    let bytes_per_iter = N;

    let mut group = c.benchmark_group("serde/encode");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(bytes_per_iter as u64));

    group.bench_function("varint7_small", |b| {
        b.iter(|| {
            let mut writer = BitWriter::with_max_capacity();
            for &v in &values {
                UnsignedVariableInteger::<7>::new(v).ser(&mut writer);
            }
            writer.to_bytes()
        })
    });
    group.finish();
}

/// Variable-length 7-bit chunks decode, values 0–127.
fn decode_varint7_small(c: &mut Criterion) {
    let values: Vec<u8> = (0..N as u8).collect();
    let bytes_per_iter = N;

    let encoded: Box<[u8]> = {
        let mut w = BitWriter::with_max_capacity();
        for &v in &values {
            UnsignedVariableInteger::<7>::new(v).ser(&mut w);
        }
        w.to_bytes()
    };

    let mut group = c.benchmark_group("serde/decode");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(bytes_per_iter as u64));

    group.bench_function("varint7_small", |b| {
        b.iter(|| {
            let mut reader = BitReader::new(&encoded);
            let mut sum = 0u64;
            for _ in 0..N {
                let v = UnsignedVariableInteger::<7>::de(&mut reader).unwrap();
                sum = sum.wrapping_add(v.get() as u64);
            }
            sum
        })
    });
    group.finish();
}

/// Variable-length 7-bit chunks, values that span two chunks (128–16383).
fn encode_varint7_large(c: &mut Criterion) {
    let values: Vec<u32> = (0..N as u32).map(|i| 128 + i * 64).collect();
    // Two-chunk values: 2 × 8 bits = 2 bytes each.
    let bytes_per_iter = N * 2;

    let mut group = c.benchmark_group("serde/encode");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(bytes_per_iter as u64));

    group.bench_function("varint7_large", |b| {
        b.iter(|| {
            let mut writer = BitWriter::with_max_capacity();
            for &v in &values {
                UnsignedVariableInteger::<7>::new(v).ser(&mut writer);
            }
            writer.to_bytes()
        })
    });
    group.finish();
}

/// Variable-length 7-bit chunks decode, two-chunk values.
fn decode_varint7_large(c: &mut Criterion) {
    let values: Vec<u32> = (0..N as u32).map(|i| 128 + i * 64).collect();
    let bytes_per_iter = N * 2;

    let encoded: Box<[u8]> = {
        let mut w = BitWriter::with_max_capacity();
        for &v in &values {
            UnsignedVariableInteger::<7>::new(v).ser(&mut w);
        }
        w.to_bytes()
    };

    let mut group = c.benchmark_group("serde/decode");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(bytes_per_iter as u64));

    group.bench_function("varint7_large", |b| {
        b.iter(|| {
            let mut reader = BitReader::new(&encoded);
            let mut sum = 0u64;
            for _ in 0..N {
                let v = UnsignedVariableInteger::<7>::de(&mut reader).unwrap();
                sum = sum.wrapping_add(v.get() as u64);
            }
            sum
        })
    });
    group.finish();
}

/// Single raw write_bit / read_bit loop — isolates the per-bit overhead.
fn raw_bit_write(c: &mut Criterion) {
    const BITS: usize = 128 * 8; // 128 bytes worth of bits, fits in 430-byte MTU
    let bytes_per_iter = BITS / 8;

    let mut group = c.benchmark_group("serde/raw");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(bytes_per_iter as u64));

    group.bench_with_input(
        BenchmarkId::new("bit_write", BITS),
        &BITS,
        |b, &bits| {
            b.iter(|| {
                let mut writer = BitWriter::with_max_capacity();
                for i in 0..bits {
                    writer.write_bit(i % 3 == 0);
                }
                writer.to_bytes()
            })
        },
    );

    group.finish();
}

fn raw_bit_read(c: &mut Criterion) {
    const BITS: usize = 128 * 8;
    let bytes_per_iter = BITS / 8;

    let encoded: Box<[u8]> = {
        let mut w = BitWriter::with_max_capacity();
        for i in 0..BITS {
            w.write_bit(i % 3 == 0);
        }
        w.to_bytes()
    };

    let mut group = c.benchmark_group("serde/raw");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Bytes(bytes_per_iter as u64));

    group.bench_with_input(
        BenchmarkId::new("bit_read", BITS),
        &BITS,
        |b, &bits| {
            b.iter(|| {
                let mut reader = BitReader::new(&encoded);
                let mut count = 0u32;
                for _ in 0..bits {
                    if reader.read_bit().unwrap() {
                        count += 1;
                    }
                }
                count
            })
        },
    );

    group.finish();
}

criterion_group!(
    name = serde_throughput;
    config = Criterion::default();
    targets =
        encode_fixed_u16,
        decode_fixed_u16,
        encode_varint7_small,
        decode_varint7_small,
        encode_varint7_large,
        decode_varint7_large,
        raw_bit_write,
        raw_bit_read,
);
