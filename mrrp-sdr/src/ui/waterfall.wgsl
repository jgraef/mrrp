


struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
}

struct VertexOutput {
    @builtin(position) fragment_position: vec4f,
}

struct FragmentOutput {
    @location(0) color: vec4f,
}

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    // draw screen-filling tri
    output.fragment_position = vec4f(
        f32((input.vertex_index & 1) << 2) - 1.0,
        f32((input.vertex_index & 2) << 1) - 1.0,
        1.0, // that's what egui_wgpu clears the depth buffer to
        1.0,
    );

    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;

    output.color = vec4f(0.0, 0.0, 0.0, 1.0);

    return output;
}
