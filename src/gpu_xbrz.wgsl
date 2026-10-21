//! The xBRZ compute shader.
//!
//! This is a transliteration of the xBRZ reference algorithm (the one the
//! `xbrz` crate implements): for every source pixel the 3x3 neighbourhood
//! decides how the corners of the block are blended, and the four rotations
//! of the neighbourhood give the four blend cases.
//!
//! The colour distance uses the same quantised YCbCr lookup table as the
//! CPU implementation, so the decisions (and therefore the pixels) are the
//! same.

struct Params {
    src_size: vec2<u32>,
    dst_size: vec2<u32>,
    scale: u32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;
@group(0) @binding(3) var<storage, read> lut: array<f32>;

const BLEND_NONE: u32 = 0u;
const BLEND_NORMAL: u32 = 1u;
const BLEND_DOMINANT: u32 = 2u;

const EQUAL_TOLERANCE: f32 = 30.0;
const CENTER_BIAS: f32 = 4.0;
const DOMINANT_THRESHOLD: f32 = 3.6;
const STEEP_THRESHOLD: f32 = 2.2;

/// A blend operation on the block: `kind` 0 blends the colour in, 1 sets it
/// and 2 does nothing.
struct Op {
    row: u32,
    col: u32,
    m: u32,
    n: u32,
    kind: u32,
}

/// The block of output pixels that one source pixel becomes.
struct Block {
    data: array<vec4<u32>, 36>,
}

/// The 4x4 neighbourhood, named like the reference implementation.
struct K4 {
    a: vec4<u32>, b: vec4<u32>, c: vec4<u32>, d: vec4<u32>,
    e: vec4<u32>, f: vec4<u32>, g: vec4<u32>, h: vec4<u32>,
    i: vec4<u32>, j: vec4<u32>, k: vec4<u32>, l: vec4<u32>,
    m: vec4<u32>, n: vec4<u32>, o: vec4<u32>, p: vec4<u32>,
}

// ---------------------------------------------------------------- the pixels

fn texel(x: i32, y: i32) -> vec4<u32> {
    if (x < 0 || y < 0 || x >= i32(params.src_size.x) || y >= i32(params.src_size.y)) {
        // Outside the image the reference implementation reads transparent
        // pixels.
        return vec4<u32>(0u, 0u, 0u, 0u);
    }
    let value = textureLoad(src, vec2<i32>(x, y), 0);
    return vec4<u32>(value * 255.0 + vec4<f32>(0.5));
}

fn same(p1: vec4<u32>, p2: vec4<u32>) -> bool {
    return all(p1 == p2);
}

/// Two colours count as different when their distance reaches the
/// tolerance.
fn neq(p1: vec4<u32>, p2: vec4<u32>) -> bool {
    return dist(p1, p2) >= EQUAL_TOLERANCE;
}

/// Two colours count as equal when their distance is below the tolerance.
fn eq(p1: vec4<u32>, p2: vec4<u32>) -> bool {
    return dist(p1, p2) < EQUAL_TOLERANCE;
}

/// The index into the colour distance table.
///
/// The differences are halved and then cut to five bits, the sign living in
/// the top bit — the same trick the reference implementation uses, so the
/// table has to be built the same way.
fn lut_index(p1: vec4<u32>, p2: vec4<u32>) -> u32 {
    let dr = (i32(p1.r) - i32(p2.r)) / 2;
    let dg = (i32(p1.g) - i32(p2.g)) / 2;
    let db = (i32(p1.b) - i32(p2.b)) / 2;
    let r8 = u32(bitcast<u32>(dr)) & 0xFFu;
    let g8 = u32(bitcast<u32>(dg)) & 0xFFu;
    let b8 = u32(bitcast<u32>(db)) & 0xFFu;
    return ((r8 >> 3u) << 10u) | ((g8 >> 3u) << 5u) | (b8 >> 3u);
}

/// The colour distance, with the alpha taken into account.
fn dist(p1: vec4<u32>, p2: vec4<u32>) -> f32 {
    let a1 = f32(p1.a) / 255.0;
    let a2 = f32(p2.a) / 255.0;
    let d = lut[lut_index(p1, p2)];
    if (a1 < a2) {
        return a1 * d + 255.0 * (a2 - a1);
    }
    return a2 * d + 255.0 * (a1 - a2);
}

// -------------------------------------------------------------- the kernel

fn load4(u: i32, v: i32) -> K4 {
    var k: K4;
    k.a = texel(u - 1, v - 1);
    k.b = texel(u, v - 1);
    k.c = texel(u + 1, v - 1);
    k.d = texel(u + 2, v - 1);
    k.e = texel(u - 1, v);
    k.f = texel(u, v);
    k.g = texel(u + 1, v);
    k.h = texel(u + 2, v);
    k.i = texel(u - 1, v + 1);
    k.j = texel(u, v + 1);
    k.k = texel(u + 1, v + 1);
    k.l = texel(u + 2, v + 1);
    k.m = texel(u - 1, v + 2);
    k.n = texel(u, v + 2);
    k.o = texel(u + 1, v + 2);
    k.p = texel(u + 2, v + 2);
    return k;
}

/// The blend type of the four corners of the block of the centre pixel.
///
/// The order is (top left, top right, bottom left, bottom right); a corner
/// is only blended when the centre pixel stands out against both of its
/// neighbours there.
fn corners_of(k: K4) -> vec4<u32> {
    let jg = ((dist(k.i, k.f) + dist(k.f, k.c)) + dist(k.n, k.k)) + dist(k.k, k.h)
        + CENTER_BIAS * dist(k.j, k.g);
    let fk = ((dist(k.e, k.j) + dist(k.j, k.o)) + dist(k.b, k.g)) + dist(k.g, k.l)
        + CENTER_BIAS * dist(k.f, k.k);

    var result = vec4<u32>(BLEND_NONE, BLEND_NONE, BLEND_NONE, BLEND_NONE);
    if (jg < fk) {
        var mode = BLEND_NORMAL;
        if (DOMINANT_THRESHOLD * jg < fk) {
            mode = BLEND_DOMINANT;
        }
        if (!same(k.f, k.g) && !same(k.f, k.j)) {
            result.x = mode;
        }
        if (!same(k.k, k.j) && !same(k.k, k.g)) {
            result.w = mode;
        }
    } else if (fk < jg) {
        var mode = BLEND_NORMAL;
        if (DOMINANT_THRESHOLD * fk < jg) {
            mode = BLEND_DOMINANT;
        }
        if (!same(k.g, k.f) && !same(k.g, k.k)) {
            result.y = mode;
        }
        if (!same(k.j, k.f) && !same(k.j, k.k)) {
            result.z = mode;
        }
    }
    return result;
}

// ------------------------------------------------- the rotated neighbourhood

fn rot_b(k: K4, r: u32) -> vec4<u32> {
    if (r == 0u) { return k.b; }
    if (r == 1u) { return k.e; }
    if (r == 2u) { return k.j; }
    return k.g;
}

fn rot_c(k: K4, r: u32) -> vec4<u32> {
    if (r == 0u) { return k.c; }
    if (r == 1u) { return k.a; }
    if (r == 2u) { return k.i; }
    return k.k;
}

fn rot_d(k: K4, r: u32) -> vec4<u32> {
    if (r == 0u) { return k.e; }
    if (r == 1u) { return k.j; }
    if (r == 2u) { return k.g; }
    return k.b;
}

fn rot_e(k: K4, r: u32) -> vec4<u32> {
    return k.f;
}

fn rot_f(k: K4, r: u32) -> vec4<u32> {
    if (r == 0u) { return k.g; }
    if (r == 1u) { return k.b; }
    if (r == 2u) { return k.e; }
    return k.j;
}

fn rot_g(k: K4, r: u32) -> vec4<u32> {
    if (r == 0u) { return k.i; }
    if (r == 1u) { return k.k; }
    if (r == 2u) { return k.c; }
    return k.a;
}

fn rot_h(k: K4, r: u32) -> vec4<u32> {
    if (r == 0u) { return k.j; }
    if (r == 1u) { return k.g; }
    if (r == 2u) { return k.b; }
    return k.e;
}

fn rot_i(k: K4, r: u32) -> vec4<u32> {
    if (r == 0u) { return k.k; }
    if (r == 1u) { return k.c; }
    if (r == 2u) { return k.a; }
    return k.i;
}

/// The four corner types, rotated.
fn rotate_blend(blend: vec4<u32>, r: u32) -> vec4<u32> {
    if (r == 0u) { return blend; }
    if (r == 1u) {
        return vec4<u32>(blend.z, blend.x, blend.w, blend.y);
    }
    if (r == 2u) {
        return vec4<u32>(blend.w, blend.z, blend.y, blend.x);
    }
    return vec4<u32>(blend.y, blend.w, blend.x, blend.z);
}

/// Where a cell of the block ends up when the block is rotated.
fn rot_index(row: u32, col: u32, scale: u32, r: u32) -> vec2<u32> {
    let last = scale - 1u;
    if (r == 0u) { return vec2<u32>(row, col); }
    if (r == 1u) { return vec2<u32>(last - col, row); }
    if (r == 2u) { return vec2<u32>(last - row, last - col); }
    return vec2<u32>(col, last - row);
}

// ----------------------------------------------------------------- blending

/// The weighted blend of the new colour into the block.
fn gradient(front: vec4<u32>, back: vec4<u32>, m: u32, n: u32) -> vec4<u32> {
    let weight_front = front.a * m;
    let weight_back = back.a * (n - m);
    let weight_sum = weight_front + weight_back;
    if (weight_sum == 0u) {
        return vec4<u32>(0u, 0u, 0u, 0u);
    }
    return vec4<u32>(
        (front.r * weight_front + back.r * weight_back) / weight_sum,
        (front.g * weight_front + back.g * weight_back) / weight_sum,
        (front.b * weight_front + back.b * weight_back) / weight_sum,
        weight_sum / n,
    );
}

// The blend tables of the reference implementation, one per scale and case.
// The cases are: shallow, steep and shallow, steep, diagonal, corner.

const OPS2_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(1u, 0u, 1u, 4u, 0u), Op(1u, 1u, 3u, 4u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
);
const OPS2_STEEP_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(1u, 0u, 1u, 4u, 0u), Op(0u, 1u, 1u, 4u, 0u), Op(1u, 1u, 5u, 6u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
);
const OPS2_STEEP: array<Op, 16> = array<Op, 16>(
    Op(0u, 1u, 1u, 4u, 0u), Op(1u, 1u, 3u, 4u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS2_DIAGONAL: array<Op, 16> = array<Op, 16>(
    Op(1u, 1u, 1u, 2u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS2_CORNER: array<Op, 16> = array<Op, 16>(
    Op(1u, 1u, 21u, 100u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);

const OPS3_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(2u, 0u, 1u, 4u, 0u), Op(1u, 2u, 1u, 4u, 0u), Op(2u, 1u, 3u, 4u, 0u),
    Op(2u, 2u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS3_STEEP_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(2u, 0u, 1u, 4u, 0u), Op(0u, 2u, 1u, 4u, 0u), Op(2u, 1u, 3u, 4u, 0u),
    Op(1u, 2u, 3u, 4u, 0u), Op(2u, 2u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS3_STEEP: array<Op, 16> = array<Op, 16>(
    Op(0u, 2u, 1u, 4u, 0u), Op(2u, 1u, 1u, 4u, 0u), Op(1u, 2u, 3u, 4u, 0u),
    Op(2u, 2u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS3_DIAGONAL: array<Op, 16> = array<Op, 16>(
    Op(1u, 2u, 1u, 8u, 0u), Op(2u, 1u, 1u, 8u, 0u), Op(2u, 2u, 7u, 8u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS3_CORNER: array<Op, 16> = array<Op, 16>(
    Op(2u, 2u, 45u, 100u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);

const OPS4_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(3u, 0u, 1u, 4u, 0u), Op(2u, 2u, 1u, 4u, 0u), Op(3u, 1u, 3u, 4u, 0u),
    Op(2u, 3u, 3u, 4u, 0u), Op(3u, 2u, 0u, 1u, 1u), Op(3u, 3u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS4_STEEP_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(3u, 1u, 3u, 4u, 0u), Op(1u, 3u, 3u, 4u, 0u), Op(3u, 0u, 1u, 4u, 0u),
    Op(0u, 3u, 1u, 4u, 0u), Op(2u, 2u, 1u, 3u, 0u), Op(3u, 3u, 0u, 1u, 1u),
    Op(3u, 2u, 0u, 1u, 1u), Op(2u, 3u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS4_STEEP: array<Op, 16> = array<Op, 16>(
    Op(0u, 3u, 1u, 4u, 0u), Op(2u, 2u, 1u, 4u, 0u), Op(1u, 3u, 3u, 4u, 0u),
    Op(3u, 2u, 3u, 4u, 0u), Op(2u, 3u, 0u, 1u, 1u), Op(3u, 3u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS4_DIAGONAL: array<Op, 16> = array<Op, 16>(
    Op(3u, 2u, 1u, 2u, 0u), Op(2u, 3u, 1u, 2u, 0u), Op(3u, 3u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS4_CORNER: array<Op, 16> = array<Op, 16>(
    Op(3u, 3u, 68u, 100u, 0u), Op(3u, 2u, 9u, 100u, 0u), Op(2u, 3u, 9u, 100u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);

const OPS5_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(4u, 0u, 1u, 4u, 0u), Op(3u, 2u, 1u, 4u, 0u), Op(2u, 4u, 1u, 4u, 0u),
    Op(4u, 1u, 3u, 4u, 0u), Op(3u, 3u, 3u, 4u, 0u), Op(4u, 2u, 0u, 1u, 1u),
    Op(4u, 3u, 0u, 1u, 1u), Op(4u, 4u, 0u, 1u, 1u), Op(3u, 4u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
);
const OPS5_STEEP_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(0u, 4u, 1u, 4u, 0u), Op(2u, 3u, 1u, 4u, 0u), Op(1u, 4u, 3u, 4u, 0u),
    Op(4u, 0u, 1u, 4u, 0u), Op(3u, 2u, 1u, 4u, 0u), Op(4u, 1u, 3u, 4u, 0u),
    Op(3u, 3u, 2u, 3u, 0u), Op(2u, 4u, 0u, 1u, 1u), Op(3u, 4u, 0u, 1u, 1u),
    Op(4u, 4u, 0u, 1u, 1u), Op(4u, 2u, 0u, 1u, 1u), Op(4u, 3u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
);
const OPS5_STEEP: array<Op, 16> = array<Op, 16>(
    Op(0u, 4u, 1u, 4u, 0u), Op(2u, 3u, 1u, 4u, 0u), Op(4u, 2u, 1u, 4u, 0u),
    Op(1u, 4u, 3u, 4u, 0u), Op(3u, 3u, 3u, 4u, 0u), Op(2u, 4u, 0u, 1u, 1u),
    Op(3u, 4u, 0u, 1u, 1u), Op(4u, 4u, 0u, 1u, 1u), Op(4u, 3u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS5_DIAGONAL: array<Op, 16> = array<Op, 16>(
    Op(4u, 2u, 1u, 8u, 0u), Op(3u, 3u, 1u, 8u, 0u), Op(2u, 4u, 1u, 8u, 0u),
    Op(4u, 3u, 7u, 8u, 0u), Op(3u, 4u, 7u, 8u, 0u), Op(4u, 4u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS5_CORNER: array<Op, 16> = array<Op, 16>(
    Op(4u, 4u, 86u, 100u, 0u), Op(4u, 3u, 23u, 100u, 0u), Op(3u, 4u, 23u, 100u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);

const OPS6_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(5u, 0u, 1u, 4u, 0u), Op(4u, 2u, 1u, 4u, 0u), Op(3u, 4u, 1u, 4u, 0u),
    Op(5u, 1u, 3u, 4u, 0u), Op(4u, 3u, 3u, 4u, 0u), Op(3u, 5u, 3u, 4u, 0u),
    Op(5u, 2u, 0u, 1u, 1u), Op(5u, 3u, 0u, 1u, 1u), Op(5u, 4u, 0u, 1u, 1u),
    Op(5u, 5u, 0u, 1u, 1u), Op(4u, 4u, 0u, 1u, 1u), Op(4u, 5u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
);
const OPS6_STEEP_SHALLOW: array<Op, 16> = array<Op, 16>(
    Op(0u, 5u, 1u, 4u, 0u), Op(2u, 4u, 1u, 4u, 0u), Op(1u, 5u, 3u, 4u, 0u),
    Op(3u, 4u, 3u, 4u, 0u), Op(5u, 0u, 1u, 4u, 0u), Op(4u, 2u, 1u, 4u, 0u),
    Op(5u, 1u, 3u, 4u, 0u), Op(4u, 3u, 3u, 4u, 0u), Op(2u, 5u, 0u, 1u, 1u),
    Op(3u, 5u, 0u, 1u, 1u), Op(4u, 5u, 0u, 1u, 1u), Op(5u, 5u, 0u, 1u, 1u),
    Op(4u, 4u, 0u, 1u, 1u), Op(5u, 4u, 0u, 1u, 1u), Op(5u, 2u, 0u, 1u, 1u),
    Op(5u, 3u, 0u, 1u, 1u),
);
const OPS6_STEEP: array<Op, 16> = array<Op, 16>(
    Op(0u, 5u, 1u, 4u, 0u), Op(2u, 4u, 1u, 4u, 0u), Op(4u, 3u, 1u, 4u, 0u),
    Op(1u, 5u, 3u, 4u, 0u), Op(3u, 4u, 3u, 4u, 0u), Op(5u, 3u, 3u, 4u, 0u),
    Op(2u, 5u, 0u, 1u, 1u), Op(3u, 5u, 0u, 1u, 1u), Op(4u, 5u, 0u, 1u, 1u),
    Op(5u, 5u, 0u, 1u, 1u), Op(4u, 4u, 0u, 1u, 1u), Op(5u, 4u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
);
const OPS6_DIAGONAL: array<Op, 16> = array<Op, 16>(
    Op(5u, 3u, 1u, 2u, 0u), Op(4u, 4u, 1u, 2u, 0u), Op(3u, 5u, 1u, 2u, 0u),
    Op(4u, 5u, 0u, 1u, 1u), Op(5u, 5u, 0u, 1u, 1u), Op(5u, 4u, 0u, 1u, 1u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u),);
const OPS6_CORNER: array<Op, 16> = array<Op, 16>(
    Op(5u, 5u, 97u, 100u, 0u), Op(4u, 5u, 42u, 100u, 0u), Op(5u, 4u, 42u, 100u, 0u),
    Op(5u, 3u, 6u, 100u, 0u), Op(3u, 5u, 6u, 100u, 0u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
    Op(0u, 0u, 0u, 1u, 2u), Op(0u, 0u, 0u, 1u, 2u),
);

/// Apply the operations of one blend case to the block.
fn apply_ops(blend_case: u32, scale: u32, rot: u32, colour: vec4<u32>, block: ptr<function, Block>) {
    var ops: array<Op, 16>;
    var count: u32 = 0u;

    if (scale == 2u) {
        if (blend_case == 0u) { ops = OPS2_SHALLOW; count = 2u; }
        else if (blend_case == 1u) { ops = OPS2_STEEP_SHALLOW; count = 3u; }
        else if (blend_case == 2u) { ops = OPS2_STEEP; count = 2u; }
        else if (blend_case == 3u) { ops = OPS2_DIAGONAL; count = 1u; }
        else { ops = OPS2_CORNER; count = 1u; }
    } else if (scale == 3u) {
        if (blend_case == 0u) { ops = OPS3_SHALLOW; count = 4u; }
        else if (blend_case == 1u) { ops = OPS3_STEEP_SHALLOW; count = 5u; }
        else if (blend_case == 2u) { ops = OPS3_STEEP; count = 4u; }
        else if (blend_case == 3u) { ops = OPS3_DIAGONAL; count = 3u; }
        else { ops = OPS3_CORNER; count = 1u; }
    } else if (scale == 4u) {
        if (blend_case == 0u) { ops = OPS4_SHALLOW; count = 6u; }
        else if (blend_case == 1u) { ops = OPS4_STEEP_SHALLOW; count = 8u; }
        else if (blend_case == 2u) { ops = OPS4_STEEP; count = 6u; }
        else if (blend_case == 3u) { ops = OPS4_DIAGONAL; count = 3u; }
        else { ops = OPS4_CORNER; count = 3u; }
    } else if (scale == 5u) {
        if (blend_case == 0u) { ops = OPS5_SHALLOW; count = 9u; }
        else if (blend_case == 1u) { ops = OPS5_STEEP_SHALLOW; count = 12u; }
        else if (blend_case == 2u) { ops = OPS5_STEEP; count = 9u; }
        else if (blend_case == 3u) { ops = OPS5_DIAGONAL; count = 6u; }
        else { ops = OPS5_CORNER; count = 3u; }
    } else {
        if (blend_case == 0u) { ops = OPS6_SHALLOW; count = 12u; }
        else if (blend_case == 1u) { ops = OPS6_STEEP_SHALLOW; count = 16u; }
        else if (blend_case == 2u) { ops = OPS6_STEEP; count = 12u; }
        else if (blend_case == 3u) { ops = OPS6_DIAGONAL; count = 6u; }
        else { ops = OPS6_CORNER; count = 5u; }
    }

    for (var index = 0u; index < count; index++) {
        let op = ops[index];
        if (op.kind == 2u) {
            continue;
        }
        let cell = rot_index(op.row, op.col, scale, rot);
        let index_in_block = cell.x * scale + cell.y;
        if (op.kind == 0u) {
            (*block).data[index_in_block] = gradient(colour, (*block).data[index_in_block], op.m, op.n);
        } else {
            (*block).data[index_in_block] = colour;
        }
    }
}

/// Blend one rotation of the neighbourhood into the block.
fn apply_rotation(
    k: K4,
    blend: vec4<u32>,
    rotation: u32,
    scale: u32,
    block: ptr<function, Block>,
) {
    let rotated = rotate_blend(blend, rotation);
    if (rotated.w == BLEND_NONE) {
        return;
    }

    let e = rot_e(k, rotation);
    let f = rot_f(k, rotation);
    let g = rot_g(k, rotation);
    let h = rot_h(k, rotation);
    let b = rot_b(k, rotation);
    let c = rot_c(k, rotation);
    let d = rot_d(k, rotation);
    let i = rot_i(k, rotation);

    var line_blend = true;
    if (rotated.w == BLEND_DOMINANT) {
        line_blend = true;
    } else {
        if (rotated.y != BLEND_NONE && neq(e, g)) { line_blend = false; }
        if (rotated.z != BLEND_NONE && neq(e, c)) { line_blend = false; }
        if (neq(e, i) && eq(g, h) && eq(h, i) && eq(i, f) && eq(f, c)) { line_blend = false; }
    }

    var colour: vec4<u32>;
    if (dist(e, f) <= dist(e, h)) {
        colour = f;
    } else {
        colour = h;
    }

    if (line_blend) {
        let fg = dist(f, g);
        let hc = dist(h, c);
        let shallow = (STEEP_THRESHOLD * fg <= hc) && neq(e, g) && neq(d, g);
        let steep = (STEEP_THRESHOLD * hc <= fg) && neq(e, c) && neq(b, c);
        if (shallow && steep) {
            apply_ops(1u, scale, rotation, colour, block);
        } else if (shallow) {
            apply_ops(0u, scale, rotation, colour, block);
        } else if (steep) {
            apply_ops(2u, scale, rotation, colour, block);
        } else {
            apply_ops(3u, scale, rotation, colour, block);
        }
    } else {
        apply_ops(4u, scale, rotation, colour, block);
    }
}

// -------------------------------------------------------------------- main

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let x = i32(gid.x);
    let y = i32(gid.y);
    if (x >= i32(params.src_size.x) || y >= i32(params.src_size.y)) {
        return;
    }
    let scale = params.scale;

    let k0 = load4(x, y);
    let k1 = load4(x, y - 1);
    let k2 = load4(x - 1, y - 1);
    let k4 = load4(x - 1, y);

    let c0 = corners_of(k0);
    let c1 = corners_of(k1);
    let c2 = corners_of(k2);
    let c4 = corners_of(k4);

    // The four corners of the block, each taken from the neighbourhood that
    // owns it.
    var blend = vec4<u32>(BLEND_NONE, BLEND_NONE, BLEND_NONE, BLEND_NONE);
    blend.x = c2.w;
    blend.y = c1.z;
    blend.w = c0.x;
    if (x == 0) {
        // The first column has no left neighbour. The reference
        // implementation takes the top left corner from the kernel to the
        // left and leaves the bottom left corner alone.
        blend.x = c4.y;
        blend.z = BLEND_NONE;
    } else {
        blend.z = c4.y;
    }

    var block: Block;
    let centre = k0.f;
    for (var index = 0u; index < 36u; index++) {
        block.data[index] = centre;
    }

    if (blend.x != BLEND_NONE || blend.y != BLEND_NONE || blend.z != BLEND_NONE
        || blend.w != BLEND_NONE) {
        for (var rotation = 0u; rotation < 4u; rotation++) {
            apply_rotation(k0, blend, rotation, scale, &block);
        }
    }

    for (var row = 0u; row < scale; row++) {
        for (var col = 0u; col < scale; col++) {
            let pixel = block.data[row * scale + col];
            let index = u32(
                (y * i32(scale) + i32(row)) * i32(params.dst_size.x) + x * i32(scale) + i32(col),
            );
            dst[index] = pack4x8unorm(vec4<f32>(pixel) / 255.0);
        }
    }
}
