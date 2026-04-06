// Terrain lightmap blend shader for Ragnarok Online maps.
//
// Applies the RO-accurate lightmap formula after PBR lighting:
//   output.rgb = output.rgb * shadow_alpha + lightmap_rgb
//
// Where:
//   - shadow_alpha (A channel) is the ambient-occlusion / shadow factor (0 = fully darkened)
//   - lightmap_rgb (RGB channels) is the baked direct-light color (additive)
//
// UV1 (in.uv_b) must be present in the mesh; it carries the per-vertex lightmap atlas UVs
// computed from GndSurface::lightmap_id.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var lightmap_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var lightmap_sampler: sampler;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    // Prepass only needs alpha-correct depth/normal output — no lightmap blending.
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

#ifdef VERTEX_UVS_B
    // Sample the lightmap atlas at UV1.
    // Applied in linear space, before tonemapping.
    let lm = textureSample(lightmap_texture, lightmap_sampler, in.uv_b);
    out.color = vec4<f32>(out.color.rgb * lm.a + lm.rgb, out.color.a);
#endif

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
