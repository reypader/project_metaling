use crate::effect_table::CylinderDef;
use crate::EffectRepeat;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::f32::consts::TAU;

/// World-unit scale applied to CYLINDER height and radius values.
/// RO cylinder coordinates are in RO units; empirically tuned.
const CYLINDER_SCALE: f32 = 5.0;

/// Number of radial segments in the cone frustum mesh.
const SEGMENTS: u32 = 12;

/// Drives per-frame updates for CYLINDER effects: optional Y rotation and texture frame cycling.
#[derive(Component)]
pub struct CylinderAnimator {
    pub animation_frames: u32,
    pub elapsed_secs: f32,
    pub frame_duration_secs: f32,
    pub rotate: bool,
    pub current_frame: u32,
    /// Remaining play count; `None` = loop forever.
    pub remaining: Option<u32>,
    /// Duration of one full play cycle in seconds. `f32::INFINITY` for static effects with no natural end.
    pub cycle_secs: f32,
    /// Time accumulated within the current play cycle.
    pub cycle_elapsed: f32,
}

pub fn spawn_cylinder_effect(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: &AssetServer,
    parent_entity: Entity,
    def: &CylinderDef,
    repeat: EffectRepeat,
) {
    let mesh = build_cone_frustum(
        def.bottom_size * CYLINDER_SCALE,
        def.top_size * CYLINDER_SCALE,
        def.height * CYLINDER_SCALE,
    );

    let tex_path = format!("tex/effect/{}.tga", def.texture_name);
    let texture: Handle<Image> = server.load(tex_path);

    let [r, g, b, a] = def.color;
    let alpha_mode = if def.blend_additive {
        AlphaMode::Add
    } else {
        AlphaMode::Blend
    };

    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        base_color: Color::srgba(r, g, b, a),
        alpha_mode,
        double_sided: true,
        cull_mode: None,
        unlit: true,
        ..default()
    });

    // Frame duration: if duration_ms is set use that / frames; otherwise fall back to 100 ms/frame.
    let frame_duration_secs = if def.animation_frames > 1 && def.duration_ms > 0.0 {
        def.duration_ms / 1000.0 / def.animation_frames as f32
    } else {
        0.1
    };

    // One cycle = total animation duration. Fall back to per-frame accumulation if duration_ms
    // is not set; static rotate-only effects have no natural end so cycle_secs = INFINITY.
    let cycle_secs = if def.duration_ms > 0.0 {
        def.duration_ms / 1000.0
    } else if def.animation_frames > 1 {
        def.animation_frames as f32 * frame_duration_secs
    } else {
        f32::INFINITY
    };

    let animator = CylinderAnimator {
        animation_frames: def.animation_frames,
        elapsed_secs: 0.0,
        frame_duration_secs,
        rotate: def.rotate,
        current_frame: 0,
        remaining: match repeat {
            EffectRepeat::Infinite => None,
            EffectRepeat::Times(n) => Some(n),
        },
        cycle_secs,
        cycle_elapsed: 0.0,
    };

    commands.entity(parent_entity).with_children(|parent| {
        parent.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::Inherited,
            animator,
        ));
    });
}

/// Advances CYLINDER animators: rotates and/or steps through texture frames each tick.
/// Despawns the parent emitter entity once all play cycles have completed.
pub fn animate_cylinders(
    mut query: Query<(
        &ChildOf,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
        &mut CylinderAnimator,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    time: Res<Time>,
) {
    for (child_of, mut transform, mat_handle, mut anim) in &mut query {
        let dt = time.delta_secs();

        if anim.rotate {
            transform.rotate_y(dt * std::f32::consts::FRAC_PI_4);
        }

        if anim.animation_frames > 1 {
            anim.elapsed_secs += dt;
            if anim.elapsed_secs >= anim.frame_duration_secs {
                anim.elapsed_secs -= anim.frame_duration_secs;
                anim.current_frame = (anim.current_frame + 1) % anim.animation_frames;

                if let Some(mat) = materials.get_mut(&mat_handle.0) {
                    let frame_height = 1.0 / anim.animation_frames as f32;
                    // V is flipped (top=0, bottom=1), so frame 0 sits at the top of the strip.
                    let v_offset = anim.current_frame as f32 * frame_height;
                    mat.uv_transform =
                        bevy::math::Affine2::from_translation(Vec2::new(0.0, v_offset))
                            * bevy::math::Affine2::from_scale(Vec2::new(1.0, frame_height));
                }
            }
        }

        if let Some(remaining) = anim.remaining {
            anim.cycle_elapsed += dt;
            if anim.cycle_elapsed >= anim.cycle_secs {
                anim.cycle_elapsed -= anim.cycle_secs;
                let next = remaining - 1;
                if next == 0 {
                    commands.entity(child_of.parent()).despawn();
                    continue;
                }
                anim.remaining = Some(next);
                anim.current_frame = 0;
                anim.elapsed_secs = 0.0;
            }
        }
    }
}

/// Builds a truncated cone (frustum) mesh with UVs that wrap around the circumference.
fn build_cone_frustum(bottom_radius: f32, top_radius: f32, height: f32) -> Mesh {
    let n = SEGMENTS;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // One extra vertex per ring to close the seam with u=1.
    let vert_count = n + 1;

    // Precompute the outward-leaning normal angle for the slanted sides.
    // The slope angle determines how much the normals tilt outward.
    let slope_len = (height * height + (bottom_radius - top_radius).powi(2)).sqrt();
    let normal_y = (bottom_radius - top_radius) / slope_len;
    let normal_xz_scale = height / slope_len;

    for i in 0..=n {
        let t = i as f32 / n as f32;
        let angle = t * TAU;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // Bottom ring
        positions.push([bottom_radius * cos_a, 0.0, bottom_radius * sin_a]);
        normals.push([normal_xz_scale * cos_a, normal_y, normal_xz_scale * sin_a]);
        uvs.push([t, 1.0]);

        // Top ring
        positions.push([top_radius * cos_a, height, top_radius * sin_a]);
        normals.push([normal_xz_scale * cos_a, normal_y, normal_xz_scale * sin_a]);
        uvs.push([t, 0.0]);
    }

    // Two triangles per quad around the circumference.
    // Vertex layout per column i: bottom = 2*i, top = 2*i+1.
    for i in 0..n {
        let b0 = 2 * i;
        let t0 = 2 * i + 1;
        let b1 = 2 * (i + 1);
        let t1 = 2 * (i + 1) + 1;

        indices.push(b0);
        indices.push(b1);
        indices.push(t0);

        indices.push(t0);
        indices.push(b1);
        indices.push(t1);
    }

    let _ = vert_count; // suppress unused warning

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
