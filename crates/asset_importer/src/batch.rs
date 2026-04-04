use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use ro_files::{gnd, rsm, rsw, translate};
use crate::manifest::Manifest;

pub fn batch(
    manifest_path: &Path,
    output_override: Option<&Path>,
    types: Option<&[String]>,
    translations: Option<&Path>,
) -> Result<()> {
    let want = |t: &str| types.is_some_and(|ts| ts.iter().any(|x| x == t));

    let known: Option<HashMap<String, String>> = match translations {
        Some(p) => Some(translate::load_known(p)?),
        None => None,
    };

    let text = std::fs::read_to_string(manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let m: Manifest =
        toml::from_str(&text).with_context(|| format!("parsing {}", manifest_path.display()))?;

    let data_root = Path::new(&m.data_root);
    let out_root: &Path = match output_override {
        Some(p) => p,
        None => Path::new(&m.output_root),
    };
    std::fs::create_dir_all(out_root)?;

    let sprite_root = out_root.join("sprite");

    let mut skip_log = String::new();
    let mut miss_log: BTreeSet<String> = BTreeSet::new();
    let mut exported = 0usize;
    let mut skipped = 0usize;

    // Mercenary entries point at shared files emitted once per gender in the manifest.
    // Track output paths already written so we skip the duplicate gender entry.
    let mut seen: HashSet<PathBuf> = HashSet::new();

    // Bodies
    if want("body") {
        for entry in &m.body {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = if let Some(merc_type) = entry.job.strip_suffix("_mercenary") {
                sprite_root.join("mercenary").join("body").join(merc_type)
            } else {
                sprite_root.join(format!("human_{}_{}", entry.gender, entry.job))
            };
            if seen.contains(&out_dir.join("body.spr")) {
                continue; // mercenary duplicate
            }
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "body", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            let imf_path = entry.imf.as_deref().map(|p| data_root.join(p));
            copy_raw(&spr_path, &act_path, imf_path.as_deref(), "body", &out_dir)?;
            seen.insert(out_dir.join("body.spr"));
            exported += 1;
        }
    }

    // Heads
    if want("head") {
        for entry in &m.head {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = sprite_root
                .join(format!("human_{}_head", entry.gender))
                .join("head");
            let name = entry.id.to_string();
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "head", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            let imf_path = entry.imf.as_deref().map(|p| data_root.join(p));
            copy_raw(&spr_path, &act_path, imf_path.as_deref(), &name, &out_dir)?;
            exported += 1;
        }
    }

    // Headgears
    if want("headgear") {
        for entry in &m.headgear {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = sprite_root
                .join(format!("human_{}_head", entry.gender))
                .join("headgear");
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "headgear", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            copy_raw(&spr_path, &act_path, None, &entry.name, &out_dir)?;
            exported += 1;
        }
    }

    // Garments
    if want("garment") {
        for entry in &m.garment {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = sprite_root
                .join(format!("human_{}_{}", entry.gender, entry.job))
                .join("garment")
                .join(&entry.name);
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "garment", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            copy_raw(&spr_path, &act_path, None, "garment", &out_dir)?;
            exported += 1;
        }
    }

    // Weapons
    if want("weapon") {
        for entry in &m.weapon {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let export_name = entry.slot.as_str();
            let out_dir = if let Some(merc_type) = entry.job.strip_suffix("_mercenary") {
                sprite_root.join("mercenary").join("body").join(merc_type).join("weapon")
            } else {
                sprite_root
                    .join(format!("human_{}_{}", entry.gender, entry.job))
                    .join("weapon")
                    .join(&entry.name)
            };
            let out_path = out_dir.join(format!("{export_name}.spr"));
            if seen.contains(&out_path) {
                continue; // mercenary duplicate
            }
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "weapon", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            copy_raw(&spr_path, &act_path, None, export_name, &out_dir)?;
            seen.insert(out_path);
            exported += 1;
        }
    }

    // Shields
    if want("shield") {
        for entry in &m.shield {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = sprite_root
                .join(format!("human_{}_{}", entry.gender, entry.job))
                .join("shield");
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "shield", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            copy_raw(&spr_path, &act_path, None, &entry.name, &out_dir)?;
            exported += 1;
        }
    }

    // Shadow
    if want("shadow") {
        for entry in &m.shadow {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = sprite_root.join("shadow");
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "shadow", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            copy_raw(&spr_path, &act_path, None, "shadow", &out_dir)?;
            exported += 1;
        }
    }

    // Projectiles
    if want("projectile") {
        for entry in &m.projectile {
            let spr_path = data_root.join(&entry.spr);
            let act_path = data_root.join(&entry.act);
            let out_dir = sprite_root.join("projectile");
            if let Some(reason) = missing(&spr_path, &act_path) {
                log_skip(&mut skip_log, "projectile", &toml::to_string(entry)?, &reason);
                skipped += 1;
                continue;
            }
            copy_raw(&spr_path, &act_path, None, &entry.name, &out_dir)?;
            exported += 1;
        }
    }

    // Sounds
    if want("sound") {
        // WAV filenames are ACT event keys and must be preserved exactly — no translation.
        let wav_src = data_root.join("wav");
        if wav_src.exists() {
            copy_dir_recursive(&wav_src, &out_root.join("wav"))?;
        }
        exported += 1;
    }

    // Maps
    if want("map") {
        // Copy shared resources. texture/ and model/ use translation-aware copy when a
        // translations map is loaded.
        let texture_src = data_root.join("texture");
        if texture_src.exists() {
            let dst = out_root.join("tex");
            if let Some(ref k) = known {
                copy_dir_translated(&texture_src, &dst, k, &mut miss_log)?;
            } else {
                copy_dir_recursive(&texture_src, &dst)?;
            }
        }

        let model_src = data_root.join("model");
        if model_src.exists() {
            let dst = out_root.join("model");
            if let Some(ref k) = known {
                copy_rsm_dir_translated(&model_src, &dst, k, &mut miss_log)?;
            } else {
                copy_dir_recursive(&model_src, &dst)?;
            }
        }

        let maps_dir = out_root.join("maps");
        for entry in &m.map {
            let rsw_path = data_root.join(&entry.rsw);
            let gnd_path = data_root.join(&entry.gnd);
            let gat_path = data_root.join(&entry.gat);
            let out_dir = maps_dir.join(&entry.name);
            if !rsw_path.exists() || !gnd_path.exists() || !gat_path.exists() {
                skip_log.push_str("# SKIPPED: missing map file(s)\n[[map]]\n");
                skip_log.push_str(&toml::to_string(entry)?);
                skip_log.push('\n');
                skipped += 1;
                continue;
            }
            std::fs::create_dir_all(&out_dir)?;
            if let Some(ref k) = known {
                let rsw_bytes = std::fs::read(&rsw_path)
                    .with_context(|| format!("reading {}", rsw_path.display()))?;
                let gnd_bytes = std::fs::read(&gnd_path)
                    .with_context(|| format!("reading {}", gnd_path.display()))?;
                let new_rsw = rsw::rewrite_model_paths(&rsw_bytes, k, &mut miss_log)
                    .with_context(|| format!("rewriting RSW {}", rsw_path.display()))?;
                let new_gnd = gnd::rewrite_textures(&gnd_bytes, k, &mut miss_log)
                    .with_context(|| format!("rewriting GND {}", gnd_path.display()))?;
                std::fs::write(out_dir.join(format!("{}.rsw", entry.name)), &new_rsw)?;
                std::fs::write(out_dir.join(format!("{}.gnd", entry.name)), &new_gnd)?;
            } else {
                std::fs::copy(&rsw_path, out_dir.join(format!("{}.rsw", entry.name)))?;
                std::fs::copy(&gnd_path, out_dir.join(format!("{}.gnd", entry.name)))?;
            }
            std::fs::copy(&gat_path, out_dir.join(format!("{}.gat", entry.name)))?;
            exported += 1;
        }
    }

    println!("Exported: {exported}  Skipped: {skipped}");

    if !skip_log.is_empty() {
        let skip_path = out_root.join("skipped.toml");
        std::fs::write(&skip_path, &skip_log)?;
        println!("Skip log: {}", skip_path.display());
    }

    if !miss_log.is_empty() {
        let miss_path = out_root.join("translation_misses.toml");
        let mut content = String::from("# Translation misses — add entries to translations.toml [known] and re-run\n[known]\n");
        for term in &miss_log {
            content.push_str(&format!("{term:?} = \"\"\n"));
        }
        std::fs::write(&miss_path, &content)?;
        println!("Translation misses ({}): {}", miss_log.len(), miss_path.display());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn missing(spr: &Path, act: &Path) -> Option<String> {
    if !spr.exists() {
        return Some(format!("spr not found: {}", spr.display()));
    }
    if !act.exists() {
        return Some(format!("act not found: {}", act.display()));
    }
    None
}

fn log_skip(log: &mut String, table: &str, entry_toml: &str, reason: &str) {
    log.push_str(&format!("# SKIPPED: {reason}\n"));
    log.push_str(&format!("[[{table}]]\n"));
    log.push_str(entry_toml);
    log.push('\n');
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&src_path, &dst.join(&name))?;
        } else if name_str.to_ascii_lowercase().ends_with(".bmp") {
            let dst_path = dst.join(bmp_to_png_name(&name_str));
            convert_bmp_to_png(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, dst.join(&name))?;
        }
    }
    Ok(())
}

fn copy_dir_translated(
    src: &Path,
    dst: &Path,
    known: &HashMap<String, String>,
    misses: &mut BTreeSet<String>,
) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let original_name = entry.file_name();
        let original_str = original_name.to_string_lossy();
        let translated_name = translate::translate_utf8_segment(&original_str, known, misses);
        let dst_name = bmp_to_png_name(&translated_name);
        let dst_path = dst.join(&dst_name);
        if entry.file_type()?.is_dir() {
            copy_dir_translated(&src_path, &dst_path, known, misses)?;
        } else if original_str.to_ascii_lowercase().ends_with(".bmp") {
            convert_bmp_to_png(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn copy_rsm_dir_translated(
    src: &Path,
    dst: &Path,
    known: &HashMap<String, String>,
    misses: &mut BTreeSet<String>,
) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let original_name = entry.file_name();
        let original_str = original_name.to_string_lossy();
        let translated_name = translate::translate_utf8_segment(&original_str, known, misses);
        if entry.file_type()?.is_dir() {
            let dst_path = dst.join(&translated_name);
            copy_rsm_dir_translated(&src_path, &dst_path, known, misses)?;
        } else if original_str.to_ascii_lowercase().ends_with(".bmp") {
            let dst_path = dst.join(bmp_to_png_name(&translated_name));
            convert_bmp_to_png(&src_path, &dst_path)?;
        } else if original_str.to_ascii_lowercase().ends_with(".rsm") {
            let dst_path = dst.join(&translated_name);
            let data = std::fs::read(&src_path)
                .with_context(|| format!("reading {}", src_path.display()))?;
            let rewritten = rsm::rewrite_textures(&data, known, misses)
                .with_context(|| format!("rewriting RSM {}", src_path.display()))?;
            std::fs::write(&dst_path, &rewritten)?;
        } else {
            let dst_path = dst.join(&translated_name);
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn copy_raw(
    spr_path: &Path,
    act_path: &Path,
    imf_path: Option<&Path>,
    name: &str,
    out_dir: &Path,
) -> Result<()> {
    std::fs::create_dir_all(out_dir)?;
    std::fs::copy(spr_path, out_dir.join(format!("{name}.spr")))?;
    std::fs::copy(act_path, out_dir.join(format!("{name}.act")))?;
    if let Some(imf) = imf_path.filter(|p| p.exists()) {
        std::fs::copy(imf, out_dir.join(format!("{name}.imf")))?;
    }
    Ok(())
}

fn bmp_to_png_name(name: &str) -> String {
    if name.to_ascii_lowercase().ends_with(".bmp") {
        format!("{}.png", &name[..name.len() - 4])
    } else {
        name.to_string()
    }
}

fn convert_bmp_to_png(src: &Path, dst: &Path) -> Result<()> {
    let img = image::open(src)
        .with_context(|| format!("opening BMP {}", src.display()))?;

    // 2. Convert to RGBA8 (gives us the alpha channel)
    let mut rgba_img = img.to_rgba8();

    // Define the color you want to make transparent (e.g., White)
    let target_color = [255, 0, 255];

    // 3. Iterate over pixels and set alpha to 0 for the target color
    for pixel in rgba_img.pixels_mut() {
        // pixel.0 is the [r, g, b, a] array
        if pixel.0[0] == target_color[0] &&
            pixel.0[1] == target_color[1] &&
            pixel.0[2] == target_color[2] {
            pixel.0[3] = 0; // Set alpha to transparent
        }
    }

    rgba_img.save(dst)
        .with_context(|| format!("saving PNG {}", dst.display()))?;
    Ok(())
}
