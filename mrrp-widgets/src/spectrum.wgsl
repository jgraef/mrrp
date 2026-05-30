
struct SpectrumConfig {
    view_matrix: mat4x4f,
    background_color: vec4f,
    background_color_signal: vec4f,
    min_db: f32,
    max_db: f32,
}

struct ColorMap {
    lut: array<vec4f>,
}

struct SpectrumData {
    start_frequency: f32,
    end_frequency: f32,
    data: array<f32>,
}

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
}

struct VertexOutput {
    @builtin(position) fragment_position: vec4f,
    @location(0) position: vec4f,
}

struct FragmentOutput {
    @location(0) color: vec4f,
}

@group(0)
@binding(0)
var<uniform> spectrum_config: SpectrumConfig;

@group(0)
@binding(1)
var<storage, read> spectrum_colormap: ColorMap;


@group(0)
@binding(2)
var<storage, read> spectrum_data: SpectrumData;

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    // draw screen-filling tri
    let vertex = vec4f(
        f32((input.vertex_index & 1) << 2) - 1.0,
        f32((input.vertex_index & 2) << 1) - 1.0,
        0.0,
        1.0,
    );

    output.fragment_position = vertex;
    output.position = spectrum_config.view_matrix * vertex;

    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;
    output.color = spectrum_config.background_color;

    let data_len = arrayLength(&spectrum_data.data);

    let k = (input.position.x - spectrum_data.start_frequency) / (spectrum_data.end_frequency - spectrum_data.start_frequency);
    if k >= 0.0 && k <= 1.0 {
        let data_index = u32(k * f32(data_len - 1));

        let value_db = linear_to_db(spectrum_data.data[data_index]);
        let is_background = step(value_db, input.position.y);

        // note that we derive the color from the dB value implied by the pixel position, not the actual value.
        // we might want to expose a way to control this behavior.
        let value_scaled = (input.position.y - spectrum_config.min_db) / (spectrum_config.max_db - spectrum_config.min_db);
        let value_clamped = clamp(value_scaled, 0.0, 1.0);
        let foreground_color = map_color(value_clamped);

        output.color = mix(foreground_color, spectrum_config.background_color_signal, is_background);
    }

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

fn map_color(t: f32) -> vec4f {
    let n = arrayLength(&(spectrum_colormap.lut));
    let x = t * f32(n - 1);

    return mix(
        spectrum_colormap.lut[clamp(u32(x), 0, n - 1)],
        spectrum_colormap.lut[clamp(u32(x + 1), 0, n - 1)],
        fract(x)
    );
}
