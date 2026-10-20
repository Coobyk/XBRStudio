//! The compute shader of the GPU backend.
//!
//! It mirrors the edge adaptive upscaling of `scalers::fsr`: the sampling
//! position follows the local gradient and the four taps per axis are
//! weighted with a cubic B-spline.

struct Params {
    src_size: vec2<u32>,
    dst_size: vec2<u32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;

/// The uniform cubic B-spline, support two.
fn bspline(x: f32) -> f32 {
    let ax = abs(x);
    if (ax < 1.0) {
        return 2.0 / 3.0 - ax * ax + ax * ax * ax / 2.0;
    }
    if (ax < 2.0) {
        let t = 2.0 - ax;
        return t * t * t / 6.0;
    }
    return 0.0;
}

/// The alpha weighted luma, so transparent texels do not fake a gradient.
fn luma(pixel: vec4<f32>) -> f32 {
    // The luma is scaled to 0..255, like the CPU implementation.
    return (0.2126 * pixel.r + 0.7152 * pixel.g + 0.0722 * pixel.b) * pixel.a * 255.0;
}

fn load(x: i32, y: i32) -> vec4<f32> {
    let width = i32(params.src_size.x);
    let height = i32(params.src_size.y);
    let cx = clamp(x, 0, width - 1);
    let cy = clamp(y, 0, height - 1);
    return textureLoad(src, vec2<i32>(cx, cy), 0);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.dst_size.x || gid.y >= params.dst_size.y) {
        return;
    }

    let scale = vec2<f32>(
        f32(params.dst_size.x) / f32(params.src_size.x),
        f32(params.dst_size.y) / f32(params.src_size.y),
    );
    let position = vec2<f32>(f32(gid.x), f32(gid.y));
    var center = (position + vec2<f32>(0.5)) / scale - vec2<f32>(0.5);

    // The edge adaptation: follow the local gradient.
    let cell = clamp(
        floor(center),
        vec2<f32>(0.0),
        vec2<f32>(f32(params.src_size.x) - 1.0, f32(params.src_size.y) - 1.0),
    );
    let cell_i = vec2<i32>(cell);
    let l00 = luma(load(cell_i.x, cell_i.y));
    let l10 = luma(load(cell_i.x + 1, cell_i.y));
    let l01 = luma(load(cell_i.x, cell_i.y + 1));
    let l11 = luma(load(cell_i.x + 1, cell_i.y + 1));

    let gradient_x = (l01 + l11) - (l00 + l10);
    let gradient_y = (l10 + l11) - (l00 + l01);
    let magnitude = max(abs(gradient_x), abs(gradient_y));
    let strength = clamp(magnitude / 64.0, 0.0, 1.0);
    var shift = vec2<f32>(0.0);
    if (gradient_x != 0.0) {
        shift.x = sign(gradient_x) * 0.25 * strength;
    }
    if (gradient_y != 0.0) {
        shift.y = sign(gradient_y) * 0.25 * strength;
    }
    center = center + shift;

    // Four taps per axis, weighted with the B-spline.
    let base = floor(center) - vec2<f32>(1.0);
    var weight_x: array<f32, 4>;
    var weight_y: array<f32, 4>;
    var total_x = 0.0;
    var total_y = 0.0;
    for (var k = 0; k < 4; k = k + 1) {
        let source_x = base.x + f32(k);
        let source_y = base.y + f32(k);
        var wx = bspline((source_x - center.x) * scale.x);
        var wy = bspline((source_y - center.y) * scale.y);
        if (source_x < 0.0 || source_x >= f32(params.src_size.x)) {
            wx = 0.0;
        }
        if (source_y < 0.0 || source_y >= f32(params.src_size.y)) {
            wy = 0.0;
        }
        weight_x[k] = wx;
        weight_y[k] = wy;
        total_x = total_x + wx;
        total_y = total_y + wy;
    }

    // The weights are normalised per axis, so the sum of the weights is one
    // in both directions.
    if (total_x > 1e-12) {
        for (var k = 0; k < 4; k = k + 1) {
            weight_x[k] = weight_x[k] / total_x;
        }
    }
    if (total_y > 1e-12) {
        for (var k = 0; k < 4; k = k + 1) {
            weight_y[k] = weight_y[k] / total_y;
        }
    }

    var colour = vec4<f32>(0.0);
    for (var j = 0; j < 4; j = j + 1) {
        if (weight_y[j] == 0.0) {
            continue;
        }
        let source_y = base.y + f32(j);
        for (var i = 0; i < 4; i = i + 1) {
            if (weight_x[i] == 0.0) {
                continue;
            }
            let source_x = base.x + f32(i);
            let weight = weight_x[i] * weight_y[j];
            let pixel = load(i32(source_x), i32(source_y));
            colour = colour + pixel * weight;
        }
    }

    let index = gid.y * params.dst_size.x + gid.x;
    dst[index] = pack4x8unorm(clamp(colour, vec4<f32>(0.0), vec4<f32>(1.0)));
}
