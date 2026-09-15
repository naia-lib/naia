//! DET1 diagnostic probe: quaternion-normalize grouping divergence (seq3842).
//!
//! Diagnostic-only: exercises arithmetic, changes no simulation, encoder, or
//! comparator. Core-only (`f32` ops + `sqrt`); no crate imports, so this file
//! runs under `cargo test`, bare `rustc --test`, and (via the test harness)
//! under node WASI for wasm32-unknown-unknown.
//!
//! Replicated formulas, exact op order, from frozen diax f9d082b1:
//! - `quat_mul_unit`: crates/math/src/physics_helpers.rs (~line 226), nalgebra
//!   left-to-right Hamilton product.
//! - `quat_normalize_unit`: same file (~line 271), `q / q.length()`.
//! - Norm groupings under test: glam-0.29.3 scalar `Vec4::dot` LTR
//!   `((x*x+y*y)+z*z)+w*w` (src/f32/scalar/vec4.rs; used on wasm32 WITHOUT
//!   simd128) vs sse2 `dot4_in_x` pairwise `(x*x+z*z)+(y*y+w*w)`
//!   (src/sse2.rs; used on x86-64). Native x86-64 therefore takes the pairwise
//!   norm, wasm the LTR norm, inside `integrate_linearized`
//!   (dynamics/.../rigid_body_components.rs), called per body per substep from
//!   `integrate_positions` (dynamics/.../solver/velocity_solver.rs:236).
//!
//! Policy: NO tolerances. Tests assert only universally-exact identities and
//! print exact u32 bits otherwise. Native-vs-wasm comparison happens ACROSS
//! runs of this same file, never inside an assert. Node-based wasm execution
//! is NOT wasmi evidence: the audited wasmi receipt must come from cybertool's
//! existing wasmi seam (one-shot export read), never from these node runs.
//!
//! Inputs are embedded as raw u32 bit patterns (exact, no decimal rounding):
//! recorded grounded-item body-rotation quats (pool=4 slot=0x10000) from the
//! frozen DET1 traces. These are POST-normalize values; true PRE-op inputs
//! need sim-side capture (requested from Brock separately).

fn bits(f: f32) -> u32 {
    f.to_bits()
}

fn f(b: u32) -> f32 {
    f32::from_bits(b)
}

/// Exact replication of diax `quat_mul_unit` (frozen f9d082b1).
fn mul_unit(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let (x1, y1, z1, w1) = (a[0], a[1], a[2], a[3]);
    let (x2, y2, z2, w2) = (b[0], b[1], b[2], b[3]);
    [
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
    ]
}

/// Scalar-backend norm (wasm32 without simd128): sqrt of LTR dot.
fn norm_ltr(q: [f32; 4]) -> f32 {
    (((q[0] * q[0]) + (q[1] * q[1])) + (q[2] * q[2]) + (q[3] * q[3])).sqrt()
}

/// SSE-backend norm (x86-64 `dot4_in_x`): sqrt of pairwise dot.
fn norm_sse(q: [f32; 4]) -> f32 {
    (((q[0] * q[0]) + (q[2] * q[2])) + ((q[1] * q[1]) + (q[3] * q[3]))).sqrt()
}

fn normalize_with(q: [f32; 4], n: f32) -> [f32; 4] {
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

fn hex4(q: [f32; 4]) -> String {
    format!(
        "[0x{:08x}, 0x{:08x}, 0x{:08x}, 0x{:08x}]",
        bits(q[0]),
        bits(q[1]),
        bits(q[2]),
        bits(q[3])
    )
}

fn ulp_dist(a: f32, b: f32) -> u32 {
    bits(a)
        .wrapping_sub(bits(b))
        .min(bits(b).wrapping_sub(bits(a)))
}

// Recorded native body-rot quats (payload g0.brot), exact bits.
const T28N: [u32; 4] = [0xbe7d2627, 0x3e7d2627, 0x3f299885, 0x3f299885];
const T28W: [u32; 4] = [0xbe7d2625, 0x3e7d2625, 0x3f299883, 0x3f299883];
const T30N: [u32; 4] = [0xbea50b32, 0x3ea50b32, 0x3f211d85, 0x3f211d85];
const T44N: [u32; 4] = [0xbf182dd9, 0x3f182dd9, 0x3ec40e6d, 0x3ec40e6d];
const IDENT: [u32; 4] = [0x00000000, 0x00000000, 0x00000000, 0x3f800000];

fn q(b: [u32; 4]) -> [f32; 4] {
    [f(b[0]), f(b[1]), f(b[2]), f(b[3])]
}

#[test]
fn mul_identity_exact() {
    let id = q(IDENT);
    for tag in [T28N, T30N, T44N] {
        let v = q(tag);
        assert_eq!(mul_unit(v, id).map(bits), tag);
        assert_eq!(mul_unit(id, v).map(bits), tag);
    }
}

#[test]
fn normalize_identity_exact() {
    let id = q(IDENT);
    assert_eq!(bits(norm_ltr(id)), 0x3f800000);
    assert_eq!(bits(norm_sse(id)), 0x3f800000);
    assert_eq!(normalize_with(id, 1.0).map(bits), IDENT);
}

#[test]
fn trace_inputs_grouping_report() {
    for (name, tag, want) in [
        ("t28", T28N, Some(T28W)),
        ("t30", T30N, None),
        ("t44", T44N, None),
    ] {
        let v = q(tag);
        let nl = norm_ltr(v);
        let ns = norm_sse(v);
        println!(
            "{} in={} n_ltr=0x{:08x} n_sse=0x{:08x} norm_ulp={}",
            name,
            hex4(v),
            bits(nl),
            bits(ns),
            ulp_dist(nl, ns)
        );
        let ol = normalize_with(v, nl);
        let os = normalize_with(v, ns);
        println!("{} out_ltr={} out_sse={}", name, hex4(ol), hex4(os));
        for i in 0..4 {
            println!(
                "{} lane{} ltr_vs_sse_ulp={}",
                name,
                i,
                ulp_dist(ol[i], os[i])
            );
        }
        if let Some(w) = want {
            let wv = q(w);
            let hit = (0..4).all(|i| bits(ol[i]) == w[i]);
            println!(
                "{} ltr_of_native_equals_recorded_wasm={} (wasm lanes {})",
                name,
                hit,
                hex4(wv)
            );
        }
    }
}

/// Import-free wasm entry: runs the scan + trace-input norms, writes exact
/// bits into OUT ([n_ltr, n_sse] per case, then scan input quats + norms).
/// Callable from JS with an empty import object; no I/O, no allocator.
static mut OUT: [u32; 32] = [0; 32];

fn emit(idx: usize, v: [f32; 4]) {
    unsafe {
        for (i, x) in v.iter().enumerate() {
            OUT[idx + i] = x.to_bits();
        }
    }
}

fn scan_first() -> Option<[f32; 4]> {
    let mut s: u32 = 0x9e3779b9;
    let base = q(T28N);
    for _ in 0..20000 {
        s ^= s.wrapping_shl(13);
        s ^= s.wrapping_shr(17);
        s ^= s.wrapping_shl(5);
        let mag = (s % 64) as f32 / f32::from_bits(0x47800000);
        let v = [
            base[0] + (s.wrapping_add(1)) as f32 * mag * 1e-9,
            base[1] - (s.wrapping_add(2)) as f32 * mag * 1e-9,
            base[2] + (s.wrapping_add(3)) as f32 * mag * 1e-9,
            base[3] - (s.wrapping_add(4)) as f32 * mag * 1e-9,
        ];
        if v.iter().all(|x| x.is_finite()) && bits(norm_ltr(v)) != bits(norm_sse(v)) {
            return Some(v);
        }
    }
    None
}

#[no_mangle]
pub extern "C" fn det1_probe_run() -> u32 {
    let mut o = 0;
    for tag in [T28N, T30N, T44N] {
        let v = q(tag);
        unsafe {
            OUT[o] = bits(norm_ltr(v));
            OUT[o + 1] = bits(norm_sse(v));
        }
        o += 2;
    }
    if let Some(v) = scan_first() {
        emit(o, v);
        unsafe {
            OUT[o + 4] = bits(norm_ltr(v));
            OUT[o + 5] = bits(norm_sse(v));
        }
        o += 6;
    }
    o as u32
}

#[no_mangle]
pub extern "C" fn det1_probe_out() -> *const u32 {
    core::ptr::addr_of!(OUT).cast::<u32>()
}

/// Deterministic xorshift32 scan for the first near-trace input where the two
/// norm groupings differ. Proves mechanism existence independent of whether
/// the recorded post-values themselves straddle a boundary.
/// QPROBE01 side-channel readout (seq3885). Consumes Brock's feature-gated
/// capture records; emits its own report stream; never touches any comparator.
///
/// PROPOSED grammar (pending Brock canonical confirmation -- parser accepts
/// exactly this shape, rejects everything else):
/// `QPROBE01 tick=<u32> timeline=<u8> slot=<u32> ord=<u32> dt=<hex8>
///  pose=<hex8>,<hex8>,<hex8>,<hex8> angvel=<hex8>,<hex8>,<hex8>
///  hang=<hex8>,<hex8>,<hex8>,<hex8> pre=<hex8>,<hex8>,<hex8>,<hex8>
///  norm=<hex8> post=<hex8>,<hex8>,<hex8>,<hex8>`
/// All floats as 8-hex-digit u32 bit patterns (exact, no decimal).
#[derive(Debug, PartialEq)]
struct QRecord {
    tick: u32,
    timeline: u8,
    slot: u32,
    ord: u32,
    dt: u32,
    pose: [u32; 4],
    angvel: [u32; 3],
    hang: [u32; 4],
    pre: [u32; 4],
    norm: u32,
    post: [u32; 4],
}

fn parse_hex8(s: &str) -> Result<u32, &'static str> {
    if s.len() != 8 {
        return Err("hex word must be 8 digits");
    }
    u32::from_str_radix(s, 16).map_err(|_| "bad hex digits")
}

fn parse_quat4(s: &str) -> Result<[u32; 4], &'static str> {
    let p: Vec<&str> = s.split(',').collect();
    if p.len() != 4 {
        return Err("quat needs 4 lanes");
    }
    Ok([
        parse_hex8(p[0])?,
        parse_hex8(p[1])?,
        parse_hex8(p[2])?,
        parse_hex8(p[3])?,
    ])
}

fn kv<'a>(tok: &'a str, key: &str) -> Result<&'a str, &'static str> {
    tok.split_once('=')
        .filter(|(k, _)| *k == key)
        .map(|(_, v)| v)
        .ok_or("bad key=value")
}

fn parse_qprobe01_line(line: &str) -> Result<QRecord, &'static str> {
    let t: Vec<&str> = line.split(' ').collect();
    if t.len() != 12 || t[0] != "QPROBE01" {
        return Err("bad tag/arity");
    }
    let av: Vec<&str> = kv(t[7], "angvel")?.split(',').collect();
    if av.len() != 3 {
        return Err("angvel needs 3 lanes");
    }
    Ok(QRecord {
        tick: kv(t[1], "tick")?.parse().map_err(|_| "bad tick")?,
        timeline: kv(t[2], "timeline")?.parse().map_err(|_| "bad timeline")?,
        slot: kv(t[3], "slot")?.parse().map_err(|_| "bad slot")?,
        ord: kv(t[4], "ord")?.parse().map_err(|_| "bad ord")?,
        dt: parse_hex8(kv(t[5], "dt")?)?,
        pose: parse_quat4(kv(t[6], "pose").map_err(|_| "bad pose")?)?,
        angvel: [parse_hex8(av[0])?, parse_hex8(av[1])?, parse_hex8(av[2])?],
        hang: parse_quat4(kv(t[8], "hang")?)?,
        pre: parse_quat4(kv(t[9], "pre")?)?,
        norm: parse_hex8(kv(t[10], "norm")?)?,
        post: parse_quat4(kv(t[11], "post")?)?,
    })
}

/// Comparison of one captured record through both norm groupings.
struct QCompare {
    n_ltr: u32,
    n_sse: u32,
    grouping_diverges: bool,
    ltr_matches_capture: bool,
    sse_matches_capture: bool,
}

/// Shared comparison core: scalar-LTR vs SSE-pairwise norm + full normalize.
/// Used by both the superseded text path and the canonical binary path.
fn compare_norm_post(pre: [u32; 4], norm: u32, post: [u32; 4]) -> QCompare {
    let v = q(pre);
    let nl = norm_ltr(v);
    let ns = norm_sse(v);
    let ol = normalize_with(v, nl).map(bits);
    let os = normalize_with(v, ns).map(bits);
    QCompare {
        n_ltr: bits(nl),
        n_sse: bits(ns),
        grouping_diverges: bits(nl) != bits(ns),
        ltr_matches_capture: bits(nl) == norm && ol == post,
        sse_matches_capture: bits(ns) == norm && os == post,
    }
}

fn compare_record(r: &QRecord) -> QCompare {
    compare_norm_post(r.pre, r.norm, r.post)
}

/// Index of the first record (file order) where scalar/SSE groupings differ.
fn earliest_grouping_mismatch(cmps: &[QCompare]) -> Option<usize> {
    cmps.iter().position(|c| c.grouping_diverges)
}

// Scan-observed diverging pre-op input (both archs agree on these bits).
const DIVERGING_PRE: [u32; 4] = [0xbe7aa08f, 0x3e7aa08f, 0x3f2a39eb, 0x3f28f71f];

fn synth_line(pre: [u32; 4], norm: u32, post: [u32; 4]) -> String {
    format!(
        "QPROBE01 tick=28 timeline=0 slot=7 ord=3 dt=3d000000 \
         pose=00000000,00000000,00000000,3f800000 angvel=00000000,00000000,00000000 \
         hang=00000000,00000000,00000000,3f800000 \
         pre={:08x},{:08x},{:08x},{:08x} norm={:08x} \
         post={:08x},{:08x},{:08x},{:08x}",
        pre[0], pre[1], pre[2], pre[3], norm, post[0], post[1], post[2], post[3]
    )
}

#[test]
fn qprobe_parser_roundtrip() {
    let line = synth_line(DIVERGING_PRE, 0x3f7fb0f3, DIVERGING_PRE);
    let r = parse_qprobe01_line(&line).expect("proposed grammar must parse");
    assert_eq!(r.tick, 28);
    assert_eq!(r.timeline, 0);
    assert_eq!(r.slot, 7);
    assert_eq!(r.ord, 3);
    assert_eq!(r.pre, DIVERGING_PRE);
    assert_eq!(r.norm, 0x3f7fb0f3);
}

#[test]
fn qprobe_parser_rejects_malformed() {
    let good = synth_line(DIVERGING_PRE, 0x3f7fb0f3, DIVERGING_PRE);
    assert!(parse_qprobe01_line("QPROBE01 tick=28").is_err());
    assert!(parse_qprobe01_line(&good.replace("QPROBE01", "XPROBE01")).is_err());
    assert!(parse_qprobe01_line(&good.replace("norm=3f7fb0f3", "norm=zzz")).is_err());
    assert!(parse_qprobe01_line(&good.replace("tick=28", "tick=xx")).is_err());
}

fn ident_record() -> QRecord {
    QRecord {
        tick: 0,
        timeline: 0,
        slot: 0,
        ord: 0,
        dt: 0,
        pose: IDENT,
        angvel: [0, 0, 0],
        hang: IDENT,
        pre: IDENT,
        norm: 0x3f800000,
        post: IDENT,
    }
}

fn diverging_record() -> QRecord {
    QRecord {
        tick: 28,
        timeline: 0,
        slot: 7,
        ord: 3,
        dt: 0,
        pose: IDENT,
        angvel: [0, 0, 0],
        hang: IDENT,
        pre: DIVERGING_PRE,
        norm: 0,
        post: DIVERGING_PRE,
    }
}

#[test]
fn qprobe_earliest_mismatch_logic() {
    let cmps: Vec<QCompare> = [ident_record(), diverging_record()]
        .iter()
        .map(compare_record)
        .collect();
    assert!(!cmps[0].grouping_diverges);
    assert!(cmps[1].grouping_diverges);
    assert_eq!(earliest_grouping_mismatch(&cmps), Some(1));
    let flat = vec![compare_record(&ident_record())];
    assert_eq!(earliest_grouping_mismatch(&flat), None);
}

#[test]
fn qprobe_readout_demo_report() {
    let r = diverging_record();
    let c = compare_record(&r);
    println!(
        "readout pre={} n_ltr=0x{:08x} n_sse=0x{:08x} diverges={} ltr_cap={} sse_cap={}",
        hex4(q(r.pre)),
        c.n_ltr,
        c.n_sse,
        c.grouping_diverges,
        c.ltr_matches_capture,
        c.sse_matches_capture
    );
}

#[test]
fn grouping_scan_report() {
    match scan_first() {
        Some(v) => println!(
            "scan first_diverging_input={} n_ltr=0x{:08x} n_sse=0x{:08x}",
            hex4(v),
            bits(norm_ltr(v)),
            bits(norm_sse(v))
        ),
        None => println!("scan found no diverging input in 20000 perturbations"),
    }
}

/// SUPERSEDED text proposal marker (seq3906/3912): the QPROBE01 text grammar
/// above (QRecord, parse_qprobe01_line, synth_line and its tests) is retained
/// for provenance only. The canonical readout path is the binary schema2
/// decoder below, built from Brock's actual encoder bytes/source
/// (cyberlith brock-dev 30b7a209 test/game/cross_arch/src/lib.rs).
const SUPERSEDED_TEXT_GRAMMAR: &str = "superseded-by-binary-schema2";

#[test]
fn superseded_text_path_label() {
    assert_eq!(SUPERSEDED_TEXT_GRAMMAR, "superseded-by-binary-schema2");
}

/// Canonical binary QPROBE01 schema2 decoder (seq3912).
///
/// Exact encoder layout (Brock cyberlith brock-dev 30b7a209):
/// magic[8] `QPROBE01` + u16 version (must be 2) + 72 buckets in fixture
/// order (timeline 0..3, ticks 24..=47). Bucket: u8 tag + u16 tick + u8
/// overflow (must be 0) + u16 count. Record, fixed 114 bytes: u32 slot,
/// u32 substep, u32 ordinal, entity flag u8 (0/1) + u64 bits (must be 0 when
/// flag is 0), did flag u8 (0/1) + u16 pool + u32 slot + u16 gen (must be
/// MAX,MAX,MAX when flag is 0), u32 dt, 4x pose, 3x angvel, 4x hang, 4x pre,
/// u32 norm, 4x post. Little-endian throughout. Exact end-consumption.
#[derive(Debug, PartialEq)]
struct QBinRecord {
    timeline: u8,
    tick: u16,
    slot: u32,
    substep: u32,
    ord: u32,
    entity: Option<u64>,
    did: Option<(u16, u32, u16)>,
    dt: u32,
    pose: [u32; 4],
    angvel: [u32; 3],
    hang: [u32; 4],
    pre: [u32; 4],
    norm: u32,
    post: [u32; 4],
}

#[derive(Debug)]
struct QBinBucket {
    timeline: u8,
    tick: u16,
    records: Vec<QBinRecord>,
}

#[derive(Debug, PartialEq)]
struct QBinErr {
    off: usize,
    msg: &'static str,
}

struct QCur<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> QCur<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], QBinErr> {
        if self.p + n > self.d.len() {
            return Err(QBinErr {
                off: self.p,
                msg: what,
            });
        }
        let b = &self.d[self.p..self.p + n];
        self.p += n;
        Ok(b)
    }
    fn u8(&mut self, what: &'static str) -> Result<u8, QBinErr> {
        Ok(self.take(1, what)?[0])
    }
    fn u16(&mut self, what: &'static str) -> Result<u16, QBinErr> {
        let b = self.take(2, what)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self, what: &'static str) -> Result<u32, QBinErr> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self, what: &'static str) -> Result<u64, QBinErr> {
        let b = self.take(8, what)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn u32x4(&mut self, what: &'static str) -> Result<[u32; 4], QBinErr> {
        Ok([
            self.u32(what)?,
            self.u32(what)?,
            self.u32(what)?,
            self.u32(what)?,
        ])
    }
}

fn decode_qbin_record(c: &mut QCur, timeline: u8, tick: u16) -> Result<QBinRecord, QBinErr> {
    let slot = c.u32("rec.slot")?;
    let substep = c.u32("rec.substep")?;
    let ord = c.u32("rec.ord")?;
    let eflag = c.u8("rec.entity_flag")?;
    if eflag > 1 {
        return Err(QBinErr {
            off: c.p - 1,
            msg: "entity flag not 0/1",
        });
    }
    let ebits = c.u64("rec.entity_bits")?;
    if eflag == 0 && ebits != 0 {
        return Err(QBinErr {
            off: c.p - 8,
            msg: "entity missing sentinel",
        });
    }
    let dflag = c.u8("rec.did_flag")?;
    if dflag > 1 {
        return Err(QBinErr {
            off: c.p - 1,
            msg: "did flag not 0/1",
        });
    }
    let pool = c.u16("rec.did.pool")?;
    let dslot = c.u32("rec.did.slot")?;
    let gen = c.u16("rec.did.gen")?;
    if dflag == 0 && (pool, dslot, gen) != (u16::MAX, u32::MAX, u16::MAX) {
        return Err(QBinErr {
            off: c.p - 8,
            msg: "did missing sentinel",
        });
    }
    Ok(QBinRecord {
        timeline,
        tick,
        slot,
        substep,
        ord,
        entity: (eflag == 1).then_some(ebits),
        did: (dflag == 1).then_some((pool, dslot, gen)),
        dt: c.u32("rec.dt")?,
        pose: c.u32x4("rec.pose")?,
        angvel: [
            c.u32("rec.angvel")?,
            c.u32("rec.angvel")?,
            c.u32("rec.angvel")?,
        ],
        hang: c.u32x4("rec.hang")?,
        pre: c.u32x4("rec.pre")?,
        norm: c.u32("rec.norm")?,
        post: c.u32x4("rec.post")?,
    })
}

fn decode_bucket(c: &mut QCur, tag: u8, tick: u16) -> Result<QBinBucket, QBinErr> {
    let t = c.u8("bucket.tag")?;
    if t != tag {
        return Err(QBinErr {
            off: c.p - 1,
            msg: "bucket tag order",
        });
    }
    let k = c.u16("bucket.tick")?;
    if k != tick {
        return Err(QBinErr {
            off: c.p - 2,
            msg: "bucket tick order",
        });
    }
    let ov = c.u8("bucket.overflow")?;
    if ov != 0 {
        return Err(QBinErr {
            off: c.p - 1,
            msg: "overflow set",
        });
    }
    let n = c.u16("bucket.count")?;
    let mut records = Vec::new();
    for _ in 0..n {
        records.push(decode_qbin_record(c, tag, tick)?);
    }
    Ok(QBinBucket {
        timeline: tag,
        tick,
        records,
    })
}

/// Full strict decode: exactly the 72 fixture buckets, exact end.
fn decode_qprobe01(d: &[u8]) -> Result<Vec<QBinBucket>, QBinErr> {
    let mut c = QCur { d, p: 0 };
    if c.take(8, "magic")? != b"QPROBE01" {
        return Err(QBinErr {
            off: 0,
            msg: "bad magic",
        });
    }
    if c.u16("version")? != 2 {
        return Err(QBinErr {
            off: 8,
            msg: "wrong version",
        });
    }
    let mut buckets = Vec::new();
    for tag in 0..3u8 {
        for tick in 24..=47u16 {
            buckets.push(decode_bucket(&mut c, tag, tick)?);
        }
    }
    if c.p != d.len() {
        return Err(QBinErr {
            off: c.p,
            msg: "trailing bytes",
        });
    }
    Ok(buckets)
}

fn bucket_index(tag: u8, tick: u16) -> Result<u32, QBinErr> {
    if tag > 2 || tick < 24 || tick > 47 {
        return Err(QBinErr {
            off: 0,
            msg: "bucket out of window",
        });
    }
    Ok(tag as u32 * 24 + (tick - 24) as u32)
}

/// Prefix decode for golden/unit streams: 1+ buckets in strictly ascending
/// fixture order from any valid start, exact end. Real captures must use
/// `decode_qprobe01` (exactly 72).
fn decode_qprobe01_prefix(d: &[u8]) -> Result<Vec<QBinBucket>, QBinErr> {
    let mut c = QCur { d, p: 0 };
    if c.take(8, "magic")? != b"QPROBE01" {
        return Err(QBinErr {
            off: 0,
            msg: "bad magic",
        });
    }
    if c.u16("version")? != 2 {
        return Err(QBinErr {
            off: 8,
            msg: "wrong version",
        });
    }
    let mut buckets = Vec::new();
    let mut prev: Option<u32> = None;
    while c.p < d.len() {
        let mark = c.p;
        let t = c.u8("bucket.tag")?;
        let k = c.u16("bucket.tick")?;
        let idx = bucket_index(t, k).map_err(|_| QBinErr {
            off: mark,
            msg: "bucket out of window",
        })?;
        if let Some(p) = prev {
            if idx <= p {
                return Err(QBinErr {
                    off: mark,
                    msg: "bucket order",
                });
            }
        }
        prev = Some(idx);
        let tag = t;
        let tick = k;
        let ov = c.u8("bucket.overflow")?;
        if ov != 0 {
            return Err(QBinErr {
                off: c.p - 1,
                msg: "overflow set",
            });
        }
        let n = c.u16("bucket.count")?;
        let mut records = Vec::new();
        for _ in 0..n {
            records.push(decode_qbin_record(&mut c, tag, tick)?);
        }
        buckets.push(QBinBucket {
            timeline: tag,
            tick,
            records,
        });
    }
    if buckets.is_empty() {
        return Err(QBinErr {
            off: c.p,
            msg: "no buckets",
        });
    }
    Ok(buckets)
}

fn compare_bin_record(r: &QBinRecord) -> QCompare {
    compare_norm_post(r.pre, r.norm, r.post)
}

/// Real-encoder golden fixture (Brock seq3923): 358 bytes, sha256
/// e09a5e8750fdfa22f1220830ab7b9d2798fcdc794c1b8b00c97650a0e48cab94,
/// worktop .../scratchpad/det1-seq3911-run5/fixture.qprobe. Single bucket
/// (tag 0, tick 24) with 3 records: missing/missing, present/missing,
/// present pool=4 slot=0x10000. Requires the prefix decoder (not full-72).
const GOLDEN_QPROBE_HEX: &str = concat!(
    "5150524f42453031020000180000030000000000000000000000000000000000000000000000ffffffffffffffff00000000",
    "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "00000000000000000000000000000000000000000000000000000000000007000000030000000200000001feffffff000000",
    "0000ffffffffffffffff00000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000090000000500",
    "00000400000001fdffffff000000000104000000010000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000",
);

fn hex_to_bytes(h: &str) -> Vec<u8> {
    let d: Vec<u8> = (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("golden hex"))
        .collect();
    d
}

#[test]
fn qbin_golden_real_encoder() {
    let d = hex_to_bytes(GOLDEN_QPROBE_HEX);
    assert_eq!(d.len(), 358);
    // Full-72 decoder must reject the single-bucket golden (exactness cuts
    // both ways); the prefix decoder accepts it.
    assert!(decode_qprobe01(&d).is_err());
    let b = decode_qprobe01_prefix(&d).expect("golden must prefix-decode");
    assert_eq!(b.len(), 1);
    assert_eq!((b[0].timeline, b[0].tick), (0, 24));
    assert_eq!(b[0].records.len(), 3);
    assert_eq!(b[0].records[0].entity, None);
    assert_eq!(b[0].records[0].did, None);
    assert_eq!(b[0].records[1].entity, Some(0xfffffffe));
    assert_eq!(b[0].records[1].did, None);
    assert_eq!(b[0].records[2].entity, Some(0xfffffffd));
    assert_eq!(b[0].records[2].did, Some((4, 0x10000, 0)));
    assert_eq!(earliest_bin_mismatch(&b), None);
}

fn earliest_bin_mismatch(buckets: &[QBinBucket]) -> Option<(u8, u16, usize)> {
    for b in buckets {
        for (i, r) in b.records.iter().enumerate() {
            if compare_norm_post(r.pre, r.norm, r.post).grouping_diverges {
                return Some((b.timeline, b.tick, i));
            }
        }
    }
    None
}

// Test-only stream builder mirroring the encoder layout (not the encoder).
fn wu8(v: &mut Vec<u8>, x: u8) {
    v.push(x);
}
fn wu16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn wu32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn wu64(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn wbucket(v: &mut Vec<u8>, tag: u8, tick: u16, ov: u8, count: u16) {
    wu8(v, tag);
    wu16(v, tick);
    wu8(v, ov);
    wu16(v, count);
}
fn wrecord(
    v: &mut Vec<u8>,
    slot: u32,
    sub: u32,
    ord: u32,
    entity: Option<u64>,
    did: Option<(u16, u32, u16)>,
    pre: [u32; 4],
    norm: u32,
    post: [u32; 4],
) {
    wu32(v, slot);
    wu32(v, sub);
    wu32(v, ord);
    match entity {
        Some(e) => {
            wu8(v, 1);
            wu64(v, e);
        }
        None => {
            wu8(v, 0);
            wu64(v, 0);
        }
    }
    match did {
        Some((p, s, g)) => {
            wu8(v, 1);
            wu16(v, p);
            wu32(v, s);
            wu16(v, g);
        }
        None => {
            wu8(v, 0);
            wu16(v, u16::MAX);
            wu32(v, u32::MAX);
            wu16(v, u16::MAX);
        }
    }
    wu32(v, 0x3d000000);
    for x in [0u32, 0, 0, 0x3f800000] {
        wu32(v, x);
    }
    for x in [0u32, 0, 0] {
        wu32(v, x);
    }
    for x in [0u32, 0, 0, 0x3f800000] {
        wu32(v, x);
    }
    for x in pre {
        wu32(v, x);
    }
    wu32(v, norm);
    for x in post {
        wu32(v, x);
    }
}

fn empty_stream() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"QPROBE01");
    wu16(&mut v, 2);
    for tag in 0..3u8 {
        for tick in 24..=47u16 {
            wbucket(&mut v, tag, tick, 0, 0);
        }
    }
    v
}

#[test]
fn qbin_empty_stream_exact() {
    let d = empty_stream();
    assert_eq!(d.len(), 10 + 72 * 6);
    let b = decode_qprobe01(&d).expect("empty stream must decode");
    assert_eq!(b.len(), 72);
    assert!(b.iter().all(|x| x.records.is_empty()));
    assert_eq!((b[0].timeline, b[0].tick), (0, 24));
    assert_eq!((b[71].timeline, b[71].tick), (2, 47));
}

#[test]
fn qbin_present_identity_roundtrip() {
    let mut d = empty_stream();
    // Insert one record into bucket (tag 0, tick 28) = bucket index 4.
    let mut rec = Vec::new();
    wrecord(
        &mut rec,
        7,
        1,
        3,
        Some(0x1122334455667788),
        Some((4, 0x10000, 0)),
        DIVERGING_PRE,
        0x3f7fb0f3,
        DIVERGING_PRE,
    );
    assert_eq!(rec.len(), 114);
    let at = 10 + 4 * 6;
    d.splice(at + 6..at + 6, rec.iter().cloned());
    d[at + 4] = 1;
    d[at + 5] = 0;
    let b = decode_qprobe01(&d).expect("single record must decode");
    let r = &b[4].records[0];
    assert_eq!(
        (r.timeline, r.tick, r.slot, r.substep, r.ord),
        (0, 28, 7, 1, 3)
    );
    assert_eq!(r.entity, Some(0x1122334455667788));
    assert_eq!(r.did, Some((4, 0x10000, 0)));
    assert_eq!(r.pre, DIVERGING_PRE);
    assert_eq!(r.norm, 0x3f7fb0f3);
    let c = compare_bin_record(r);
    assert!(c.grouping_diverges);
    assert_eq!(earliest_bin_mismatch(&b), Some((0, 28, 0)));
}

#[test]
fn qbin_missing_identity_roundtrip() {
    let mut d = empty_stream();
    let mut rec = Vec::new();
    wrecord(&mut rec, 9, 2, 5, None, None, IDENT, 0x3f800000, IDENT);
    let at = 10 + 4 * 6;
    d.splice(at + 6..at + 6, rec.iter().cloned());
    d[at + 4] = 1;
    d[at + 5] = 0;
    let b = decode_qprobe01(&d).expect("missing-identity record must decode");
    let r = &b[4].records[0];
    assert_eq!(r.entity, None);
    assert_eq!(r.did, None);
    assert!(!compare_bin_record(r).grouping_diverges);
}

#[test]
fn qbin_rejects_corrupt() {
    let good = empty_stream();
    // Bad magic.
    let mut d = good.clone();
    d[0] = b'X';
    assert_eq!(decode_qprobe01(&d).unwrap_err().msg, "bad magic");
    // Wrong version.
    let mut d = good.clone();
    d[8] = 1;
    d[9] = 0;
    assert_eq!(decode_qprobe01(&d).unwrap_err().msg, "wrong version");
    // Truncated mid-stream.
    assert!(decode_qprobe01(&good[..100]).is_err());
    // Trailing byte.
    let mut d = good.clone();
    d.push(0);
    assert_eq!(decode_qprobe01(&d).unwrap_err().msg, "trailing bytes");
    // Overflow set on first bucket (offset 10+3).
    let mut d = good.clone();
    d[13] = 1;
    assert_eq!(decode_qprobe01(&d).unwrap_err().msg, "overflow set");
    // Entity flag 2 (first bucket has count 0; build one-record stream).
    let mut d = empty_stream();
    let mut rec = Vec::new();
    wrecord(
        &mut rec,
        0,
        0,
        0,
        Some(1),
        Some((1, 2, 3)),
        IDENT,
        0x3f800000,
        IDENT,
    );
    let at = 10;
    d.splice(at + 6..at + 6, rec.iter().cloned());
    d[at + 4] = 1;
    d[at + 5] = 0;
    let mut bad = d.clone();
    bad[at + 6 + 12] = 2;
    assert_eq!(
        decode_qprobe01(&bad).unwrap_err().msg,
        "entity flag not 0/1"
    );
    // Tag order swapped.
    let mut d = good.clone();
    d[10] = 1;
    assert_eq!(decode_qprobe01(&d).unwrap_err().msg, "bucket tag order");
    // Tick order wrong.
    let mut d = good.clone();
    d[11] = 25;
    d[12] = 0;
    assert_eq!(decode_qprobe01(&d).unwrap_err().msg, "bucket tick order");
    // Entity-missing sentinel nonzero.
    let mut d = empty_stream();
    let mut rec = Vec::new();
    wrecord(&mut rec, 0, 0, 0, None, None, IDENT, 0x3f800000, IDENT);
    let at = 10;
    d.splice(at + 6..at + 6, rec.iter().cloned());
    d[at + 4] = 1;
    d[at + 5] = 0;
    let mut bad = d.clone();
    bad[at + 6 + 12 + 1] = 9;
    assert_eq!(
        decode_qprobe01(&bad).unwrap_err().msg,
        "entity missing sentinel"
    );
    // Did-missing sentinel non-MAX.
    let mut bad = d.clone();
    bad[at + 6 + 12 + 9 + 1] = 0;
    assert_eq!(
        decode_qprobe01(&bad).unwrap_err().msg,
        "did missing sentinel"
    );
    // Count overrun claims records beyond end.
    let mut d = good.clone();
    d[14] = 5;
    d[15] = 0;
    assert!(decode_qprobe01(&d).is_err());
}

/// Actual tick-28 substep-3 pre-op quat (grounded body pool=4 slot=0x10000),
/// identical bit-for-bit in native + wasmi captures. Norms measured:
/// native (SSE backend) 0x3f80041d, wasmi (scalar backend) 0x3f80041e.
const ACTUAL_PRE: [u32; 4] = [0xbe7d2e4a, 0x3e7d2e4a, 0x3f299df8, 0x3f299df8];
const ACTUAL_NORM_SSE: u32 = 0x3f80041d;
const ACTUAL_NORM_LTR: u32 = 0x3f80041e;
const ACTUAL_POST_SSE: [u32; 4] = [0xbe7d2627, 0x3e7d2627, 0x3f299885, 0x3f299885];

#[test]
fn actual_pre_grouping_verdict() {
    let v = q(ACTUAL_PRE);
    let nl = norm_ltr(v);
    let ns = norm_sse(v);
    assert_eq!(bits(nl), ACTUAL_NORM_LTR);
    assert_eq!(bits(ns), ACTUAL_NORM_SSE);
    assert_eq!(normalize_with(v, ns).map(bits), ACTUAL_POST_SSE);
    assert_ne!(bits(nl), bits(ns));
}

/// Failing-before regression DESIGN (for diax-side placement by the
/// production-correction owner; delineated here so the gate contract is
/// exact). Test `quat_normalize_pins_tick28_pre` calls the REAL
/// `quat_normalize_unit(ACTUAL_PRE)` and asserts output bits ==
/// ACTUAL_POST_SSE. Pre-fix it FAILS on wasm32 (backend computes LTR ->
/// 0x3f80041e path) and passes native; post-fix (explicit unified norm
/// order) it passes on both archs and pins the unified behavior against
/// future regrouping. Recommended fix (smallest): replace the
/// backend-dependent `q.length()` in diax `quat_normalize_unit`
/// (crates/math/src/physics_helpers.rs) with the explicit pairwise norm
/// `((x*x + z*z) + (y*y + w*w)).sqrt()`, matching nalgebra order and current
/// native behavior; the scalar-LTR alternative also unifies but abandons
/// nalgebra parity. Audit sibling per-step `.length()`/`.normalize()` calls
/// on rotation state for the same hazard class.
#[test]
fn regression_design_pins() {
    // The pinned values this design rests on, verified on real captures.
    assert_eq!(
        ACTUAL_POST_SSE,
        [0xbe7d2627, 0x3e7d2627, 0x3f299885, 0x3f299885]
    );
}

#[test]
fn qbin_earliest_mismatch_binary_logic() {
    let mut d = empty_stream();
    let mut r0 = Vec::new();
    wrecord(&mut r0, 0, 0, 0, None, None, IDENT, 0x3f800000, IDENT);
    let mut r1 = Vec::new();
    wrecord(
        &mut r1,
        7,
        1,
        3,
        Some(5),
        Some((4, 0x10000, 0)),
        DIVERGING_PRE,
        0,
        DIVERGING_PRE,
    );
    // Bucket (0,28) index 4 gets identity then diverging; bucket (1,28) gets none.
    let at = 10 + 4 * 6;
    let mut ins = Vec::new();
    ins.extend_from_slice(&r0);
    ins.extend_from_slice(&r1);
    d.splice(at + 6..at + 6, ins.iter().cloned());
    d[at + 4] = 2;
    d[at + 5] = 0;
    let b = decode_qprobe01(&d).expect("two-record bucket must decode");
    assert_eq!(earliest_bin_mismatch(&b), Some((0, 28, 1)));
    let e = decode_qprobe01(&empty_stream()).expect("empty decodes");
    assert_eq!(earliest_bin_mismatch(&e), None);
}
