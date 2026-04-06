use bevy::{asset::uuid_handle, pbr::{ExtendedMaterial, MaterialExtension}, prelude::*, render::render_resource::AsBindGroup, shader::ShaderRef};

/// Type alias for the terrain material with RO-accurate lightmap blending.
pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainLightmapExtension>;

pub const TERRAIN_LIGHTMAP_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("726f5f74-6c6d-6170-0000-000000000001");

/// `StandardMaterial` extension that applies the RO lightmap blend in the fragment shader.
///
/// Blend formula (post-PBR lighting, pre-tonemapping):
///   `output.rgb = output.rgb * shadow_alpha + lightmap_rgb`
///
/// The lightmap atlas (`lightmap` field) must be an `Rgba8UnormSrgb` image where:
///   - RGB = baked direct-light color (additive contribution)
///   - A   = shadow / ambient-occlusion factor (multiplicative attenuation)
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct TerrainLightmapExtension {
    /// Lightmap atlas. RGBA: RGB = baked color, A = shadow/AO.
    #[texture(100)]
    #[sampler(101)]
    pub lightmap: Handle<Image>,
}

impl MaterialExtension for TerrainLightmapExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_LIGHTMAP_SHADER_HANDLE.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TERRAIN_LIGHTMAP_SHADER_HANDLE.into()
    }
}
