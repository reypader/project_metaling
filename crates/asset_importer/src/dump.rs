use ro_files::act::{ActAction, ActFile, ActFrame, ActSprite};
use ro_files::spr::SprFile;

const PLAYER_BASES: &[(usize, &str)] = &[
    (0, "stand"),
    (8, "walk"),
    (16, "sit"),
    (24, "pickup"),
    (32, "atk_wait"),
    (40, "attack"),
    (48, "damage"),
    (56, "damage2"),
    (64, "dead"),
    (72, "unk"),
    (80, "attack2"),
    (88, "attack3"),
    (96, "skill"),
];

const MONSTER_BASES: &[(usize, &str)] = &[
    (0, "stand"),
    (8, "move"),
    (16, "attack"),
    (24, "damage"),
    (32, "dead"),
    (40, "unknown_1"),
    (48, "unknown_2"),
    (56, "unknown_3"),
    (64, "unknown_4"),
    (72, "unknown_5"),
];

const DIRS: &[&str] = &["s", "sw", "w", "nw", "n", "ne", "e", "se"];

fn action_label(idx: usize, total_actions: usize) -> String {
    let base = idx - (idx % 8);
    let dir = idx % 8;
    let bases: &[(usize, &str)] = if total_actions != 104 && total_actions.is_multiple_of(8) {
        MONSTER_BASES
    } else {
        PLAYER_BASES
    };
    if let Some(&(_, name)) = bases.iter().find(|&&(b, _)| b == base) {
        format!("{}_{}", name, DIRS[dir])
    } else {
        format!("action_{idx:03}_{}", DIRS[dir])
    }
}

fn has_visible_sprite(act: &ActFile, action_idx: usize) -> bool {
    let action = &act.actions[action_idx];
    action.frames.iter().any(|f| {
        f.sprites
            .iter()
            .any(|s| s.spr_id >= 0 && s.x_scale.abs() > 1e-6)
    })
}

/// Compute bounding box for a single frame's layers in feet-origin space.
/// Returns None if no visible layers.
fn frame_bounds(spr: &SprFile, frame: &ActFrame) -> Option<(i32, i32, i32, i32)> {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for layer in &frame.sprites {
        if layer.spr_id < 0 {
            continue;
        }
        let Some(img) = spr.get_image(layer.spr_id, layer.spr_type) else {
            continue;
        };
        let w = img.width as f32;
        let h = img.height as f32;
        let (sx, sy) = effective_scale(layer);
        let angle = (layer.rotation as f32).to_radians();
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let tx = layer.x as f32;
        let ty = layer.y as f32;

        for (u, v) in [
            (-w / 2.0, -h / 2.0),
            (w / 2.0, -h / 2.0),
            (w / 2.0, h / 2.0),
            (-w / 2.0, h / 2.0),
        ] {
            let cx = (cos_a * sx * u - sin_a * sy * v + tx).round() as i32;
            let cy = (sin_a * sx * u + cos_a * sy * v + ty).round() as i32;
            min_x = min_x.min(cx);
            min_y = min_y.min(cy);
            max_x = max_x.max(cx);
            max_y = max_y.max(cy);
        }
    }

    if min_x == i32::MAX {
        None
    } else {
        Some((min_x, min_y, max_x, max_y))
    }
}

fn effective_scale(layer: &ActSprite) -> (f32, f32) {
    let flip = if layer.flags & 1 != 0 {
        -1.0f32
    } else {
        1.0f32
    };
    (layer.x_scale * flip, layer.y_scale)
}

/// Print which action indices have at least one visible sprite layer.
pub fn scan(act: &ActFile) {
    println!(
        "ACT v{:#06x}: {} actions, {} events",
        act.version,
        act.actions.len(),
        act.events.len()
    );
    println!();
    println!("Actions with visible sprites:");

    let mut last_base: Option<usize> = None;
    for i in 0..act.actions.len() {
        if !has_visible_sprite(act, i) {
            continue;
        }
        let base = i - (i % 8);
        if last_base != Some(base) {
            println!();
            last_base = Some(base);
        }
        let n_frames = act.actions[i].frames.len();
        println!(
            "  {:3}  {}  ({} frames)",
            i,
            action_label(i, act.actions.len()),
            n_frames
        );
    }
}

/// Dump per-frame layer and attach-point data for the given actions.
/// If `action_filter` is None, dumps all actions that have visible sprites.
pub fn dump(act: &ActFile, spr: Option<&SprFile>, action_filter: Option<&[usize]>) {
    println!(
        "ACT v{:#06x}: {} actions, {} events",
        act.version,
        act.actions.len(),
        act.events.len()
    );

    // Compute global canvas info once if SPR is available.
    let canvas_info: Option<(i32, i32, i32, i32)> = spr.map(|spr| {
        let pad = 4i32;
        let (min_x, min_y, max_x, max_y) = compute_bounds(spr, &act.actions);
        let canvas_w = ((max_x - min_x) + pad * 2).max(1);
        let canvas_h = ((max_y - min_y) + pad * 2).max(1);
        let origin_x = pad - min_x;
        let origin_y = pad - min_y;
        println!(
            "Canvas: {canvas_w}×{canvas_h}px  origin=({origin_x},{origin_y})  \
             bounds=({min_x},{min_y})..({max_x},{max_y})"
        );
        (canvas_w, canvas_h, origin_x, origin_y)
    });

    if !act.events.is_empty() {
        println!("Events:");
        for (i, e) in act.events.iter().enumerate() {
            println!("  [{i}] {e:?}");
        }
    }

    let indices: Vec<usize> = match action_filter {
        Some(f) => f
            .iter()
            .copied()
            .filter(|&i| i < act.actions.len())
            .collect(),
        None => (0..act.actions.len())
            .filter(|&i| has_visible_sprite(act, i))
            .collect(),
    };

    for action_idx in indices {
        let action = &act.actions[action_idx];
        println!(
            "\n=== action {:3}  {}  ({} frames, interval={:.0}ms) ===",
            action_idx,
            action_label(action_idx, act.actions.len()),
            action.frames.len(),
            action.frame_ms(),
        );

        for (fi, frame) in action.frames.iter().enumerate() {
            let event = if frame.event_id >= 0 {
                format!(
                    "  event={} ({:?})",
                    frame.event_id,
                    act.events
                        .get(frame.event_id as usize)
                        .map(|s| s.as_str())
                        .unwrap_or("?")
                )
            } else {
                String::new()
            };

            // Attach points: feet-origin, and canvas-space if origin is known.
            let attach_str = if frame.attach_points.is_empty() {
                String::new()
            } else {
                let pts: Vec<String> = frame
                    .attach_points
                    .iter()
                    .map(|p| {
                        if let Some((_, _, ox, oy)) = canvas_info {
                            format!("({},{}) → canvas({},{})", p.x, p.y, ox + p.x, oy + p.y)
                        } else {
                            format!("({},{})", p.x, p.y)
                        }
                    })
                    .collect();
                format!("  attach=[{}]", pts.join(", "))
            };

            // Per-frame natural bounds vs shared canvas.
            let bounds_str = if let Some(spr) = spr {
                if let Some((fmin_x, fmin_y, fmax_x, fmax_y)) = frame_bounds(spr, frame) {
                    let fw = fmax_x - fmin_x;
                    let fh = fmax_y - fmin_y;
                    if let Some((cw, ch, _, _)) = canvas_info {
                        format!(
                            "  frame_bounds=({fmin_x},{fmin_y})..({fmax_x},{fmax_y}) {fw}×{fh}  \
                             dead_space={}×{}",
                            cw - fw,
                            ch - fh
                        )
                    } else {
                        format!("  frame_bounds=({fmin_x},{fmin_y})..({fmax_x},{fmax_y}) {fw}×{fh}")
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            println!("  frame {:2}:{}{}{}", fi, event, attach_str, bounds_str);

            for (si, s) in frame.sprites.iter().enumerate() {
                let flip = if s.flags & 1 != 0 { " flip" } else { "" };
                let scale = if (s.x_scale - s.y_scale).abs() < 1e-4 {
                    format!("{:.3}", s.x_scale)
                } else {
                    format!("{:.3}x{:.3}", s.x_scale, s.y_scale)
                };
                let rot = if s.rotation != 0 {
                    format!(" rot={}", s.rotation)
                } else {
                    String::new()
                };
                let tint = if s.tint != [255, 255, 255, 255] {
                    format!(" tint={:?}", s.tint)
                } else {
                    String::new()
                };
                let img_size = spr
                    .and_then(|spr| spr.get_image(s.spr_id, s.spr_type))
                    .map(|img| format!(" img={}×{}", img.width, img.height))
                    .unwrap_or_default();
                println!(
                    "    layer {:2}: spr_id={:4} type={} x={:4} y={:4} scale={}{}{}{}{}",
                    si, s.spr_id, s.spr_type, s.x, s.y, scale, rot, flip, tint, img_size
                );
            }
        }
    }
}

/// Compute the bounding box (min_x, min_y, max_x, max_y) in sprite-space
/// covering all transformed layer corners across all frames in all actions.
fn compute_bounds(spr: &SprFile, actions: &[ActAction]) -> (i32, i32, i32, i32) {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for action in actions {
        for frame in &action.frames {
            for layer in &frame.sprites {
                if layer.spr_id < 0 {
                    continue;
                }
                let Some(img) = spr.get_image(layer.spr_id, layer.spr_type) else {
                    continue;
                };
                let w = img.width as f32;
                let h = img.height as f32;
                let flip = if layer.flags & 1 != 0 { -1.0f32 } else { 1.0 };
                let sx = layer.x_scale * flip;
                let sy = layer.y_scale;
                let angle = (layer.rotation as f32).to_radians();
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let tx = layer.x as f32;
                let ty = layer.y as f32;
                for (u, v) in [
                    (-w / 2.0, -h / 2.0),
                    (w / 2.0, -h / 2.0),
                    (w / 2.0, h / 2.0),
                    (-w / 2.0, h / 2.0),
                ] {
                    let cx = (cos_a * sx * u - sin_a * sy * v + tx).round() as i32;
                    let cy = (sin_a * sx * u + cos_a * sy * v + ty).round() as i32;
                    min_x = min_x.min(cx);
                    min_y = min_y.min(cy);
                    max_x = max_x.max(cx);
                    max_y = max_y.max(cy);
                }
            }
        }
    }

    if min_x == i32::MAX {
        (0, 0, 0, 0)
    } else {
        (min_x, min_y, max_x, max_y)
    }
}
