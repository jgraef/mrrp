
struct SpectrumConfig {
    min_db: f32,
    max_db: f32,
    //_padding: [u32; 2],
    background_color: vec4f,
    foreground_color1: vec4f,
    foreground_color2: vec4f,
}

struct SpectrumData {
    data: array<f32>,
}

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
}

struct VertexOutput {
    @builtin(position) fragment_position: vec4f,
    @location(0) uv: vec2f,
}

struct FragmentOutput {
    @location(0) color: vec4f,
}

@group(0)
@binding(0)
var<uniform> spectrum_config: SpectrumConfig;

@group(0)
@binding(1)
var<storage, read> spectrum_data: SpectrumData;

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.uv = vec2f(
        f32((input.vertex_index & 1) << 1),
        f32((input.vertex_index & 2))
    );

    // draw screen-filling tri
    output.fragment_position = vec4f(
        output.uv * 2.0 - 1.0,
        1.0, // that's what egui_wgpu clears the depth buffer to
        1.0,
    );

    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;

    let data_len = arrayLength(&spectrum_data.data);
    let index = u32(input.uv.x * f32(data_len - 1));
    let value = (linear_to_db(spectrum_data.data[index]) - spectrum_config.min_db) / (spectrum_config.max_db - spectrum_config.min_db);

    let y = input.uv.y;
    let is_background = step(value, y);

    let foreground_color = mix(spectrum_config.foreground_color1, spectrum_config.foreground_color2, y / value);

    output.color = mix(foreground_color, spectrum_config.background_color, is_background);

    return output;
}

// converts linear scale to dB
fn linear_to_db(value: f32) -> f32 {
    return 10.0 * log10(value);
}

fn log10(value: f32) -> f32 {
    // uses change-of-base identity to implement log10
    // note: we think log2 is faster than ln, because IEEE-754 floats can do this pretty well.

    const LOG2_10: f32 = 1.0 / log2(10.0);
    return log2(value) * LOG2_10;
}
