use anyhow::{Context, Result};
use ro_files::{gnd, rsm, rsw, str as ro_str, translate};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use crate::classify::Manifest;
use ro_files::Grf;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn export<R: Read + Seek>(
    grf: &mut Grf<R>,
    translated_paths: &[String],
    manifest: &Manifest,
    output: &Path,
    known: &Option<HashMap<String, String>>,
    verbose: bool,
) -> Result<()> {
    fs::create_dir_all(output)?;
    let sprite_root = output.join("sprite");
    let human_root = sprite_root.join("human");

    let mut skip_log = String::new();
    let mut miss_log: BTreeSet<String> = BTreeSet::new();
    let mut exported = 0usize;
    let mut skipped = 0usize;

    // Track output paths already written (mercenary dedup).
    let mut seen: HashSet<PathBuf> = HashSet::new();

    // Bodies
    for entry in &manifest.body {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "body", &format!("{}_{}", entry.job, entry.gender), "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let out_dir = if let Some(merc_type) = entry.job.strip_suffix("_mercenary") {
            sprite_root.join("mercenary").join("body").join(merc_type)
        } else {
            human_root.join(format!("{}_{}", entry.gender, entry.job))
        };
        if seen.contains(&out_dir.join("body.spr")) {
            continue;
        }
        fs::create_dir_all(&out_dir)?;
        fs::write(out_dir.join("body.spr"), &spr_data)?;
        fs::write(out_dir.join("body.act"), &act_data)?;
        if let Some(imf_idx) = entry.imf_idx
            && let Some(imf_data) = read_entry(grf, imf_idx)
        {
            fs::write(out_dir.join("body.imf"), &imf_data)?;
        }
        seen.insert(out_dir.join("body.spr"));
        if verbose {
            println!("body: {}_{}", entry.job, entry.gender);
        }
        exported += 1;
    }

    // Heads
    for entry in &manifest.head {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "head", &format!("{}_{}", entry.id, entry.gender), "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let out_dir = human_root
            .join(format!("{}_head", entry.gender))
            .join("head");
        let name = entry.id.to_string();
        fs::create_dir_all(&out_dir)?;
        fs::write(out_dir.join(format!("{name}.spr")), &spr_data)?;
        fs::write(out_dir.join(format!("{name}.act")), &act_data)?;
        if verbose {
            println!("head: {}_{}", entry.id, entry.gender);
        }
        exported += 1;
    }

    // Headgears
    for entry in &manifest.headgear {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "headgear", &format!("{}_{}", entry.name, entry.gender), "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let out_dir = human_root
            .join(format!("{}_head", entry.gender))
            .join("headgear");
        fs::create_dir_all(&out_dir)?;
        fs::write(out_dir.join(format!("{}.spr", entry.name)), &spr_data)?;
        fs::write(out_dir.join(format!("{}.act", entry.name)), &act_data)?;
        if verbose {
            println!("headgear: {}_{}", entry.name, entry.gender);
        }
        exported += 1;
    }

    // Garments
    for entry in &manifest.garment {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "garment", &format!("{}_{}_{}", entry.name, entry.job, entry.gender), "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let out_dir = human_root
            .join(format!("{}_{}", entry.gender, entry.job))
            .join("garment")
            .join(&entry.name);
        fs::create_dir_all(&out_dir)?;
        fs::write(out_dir.join("garment.spr"), &spr_data)?;
        fs::write(out_dir.join("garment.act"), &act_data)?;
        if verbose {
            println!("garment: {}_{}_{}", entry.name, entry.job, entry.gender);
        }
        exported += 1;
    }

    // Weapons
    for entry in &manifest.weapon {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "weapon", &format!("{}_{}_{}", entry.name, entry.job, entry.gender), "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let export_name = entry.slot.as_str();
        let out_dir = if let Some(merc_type) = entry.job.strip_suffix("_mercenary") {
            sprite_root
                .join("mercenary")
                .join("body")
                .join(merc_type)
                .join("weapon")
        } else {
            human_root
                .join(format!("{}_{}", entry.gender, entry.job))
                .join("weapon")
                .join(&entry.name)
        };
        let out_path = out_dir.join(format!("{export_name}.spr"));
        if seen.contains(&out_path) {
            continue;
        }
        fs::create_dir_all(&out_dir)?;
        fs::write(&out_path, &spr_data)?;
        fs::write(out_dir.join(format!("{export_name}.act")), &act_data)?;
        seen.insert(out_path);
        if verbose {
            println!("weapon: {}_{}_{}", entry.name, entry.job, entry.gender);
        }
        exported += 1;
    }

    // Shields
    for entry in &manifest.shield {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "shield", &format!("{}_{}_{}", entry.name, entry.job, entry.gender), "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let out_dir = human_root
            .join(format!("{}_{}", entry.gender, entry.job))
            .join("shield");
        fs::create_dir_all(&out_dir)?;
        fs::write(out_dir.join(format!("{}.spr", entry.name)), &spr_data)?;
        fs::write(out_dir.join(format!("{}.act", entry.name)), &act_data)?;
        if verbose {
            println!("shield: {}_{}_{}", entry.name, entry.job, entry.gender);
        }
        exported += 1;
    }

    // Monsters
    for entry in &manifest.monster {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "monster", &entry.name, "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let out_dir = sprite_root.join("monster").join(&entry.name);
        fs::create_dir_all(&out_dir)?;
        fs::write(out_dir.join("body.spr"), &spr_data)?;
        fs::write(out_dir.join("body.act"), &act_data)?;
        if verbose {
            println!("monster: {}", entry.name);
        }
        exported += 1;
    }

    // Shadow
    for entry in &manifest.shadow {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "shadow", "shadow", "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let out_dir = sprite_root.join("shadow");
        fs::create_dir_all(&out_dir)?;
        fs::write(out_dir.join("shadow.spr"), &spr_data)?;
        fs::write(out_dir.join("shadow.act"), &act_data)?;
        if verbose {
            println!("shadow");
        }
        exported += 1;
    }

    // Projectiles
    for entry in &manifest.projectile {
        let spr_data = read_entry(grf, entry.spr_idx);
        let act_data = read_entry(grf, entry.act_idx);
        let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
            log_skip(&mut skip_log, "projectile", &entry.name, "missing spr/act in GRF");
            skipped += 1;
            continue;
        };
        let out_dir = sprite_root.join("projectile");
        fs::create_dir_all(&out_dir)?;
        fs::write(out_dir.join(format!("{}.spr", entry.name)), &spr_data)?;
        fs::write(out_dir.join(format!("{}.act", entry.name)), &act_data)?;
        if verbose {
            println!("projectile: {}", entry.name);
        }
        exported += 1;
    }

    // Effect sprites
    {
        let effect_out = sprite_root.join("effect");
        for entry in &manifest.effect {
            let spr_data = read_entry(grf, entry.spr_idx);
            let act_data = read_entry(grf, entry.act_idx);
            let (Some(spr_data), Some(act_data)) = (spr_data, act_data) else {
                log_skip(&mut skip_log, "effect", &entry.name, "missing spr/act in GRF");
                skipped += 1;
                continue;
            };
            fs::create_dir_all(&effect_out)?;
            fs::write(effect_out.join(format!("{}.spr", entry.name)), &spr_data)?;
            fs::write(effect_out.join(format!("{}.act", entry.name)), &act_data)?;
            if verbose {
                println!("effect: {}", entry.name);
            }
            exported += 1;
        }

    }

    // Lookup (text data)
    {
        let misc_out = output.join("misc");
        for entry in &manifest.lookup {
            let Some(data) = read_entry(grf, entry.idx) else {
                log_skip(&mut skip_log, "lookup", &entry.path, "missing in GRF");
                skipped += 1;
                continue;
            };
            let filename = Path::new(&entry.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if filename == "mp3nametable.txt" {
                let map = parse_mp3nametable(&data)?;
                let json = serde_json::to_string_pretty(&map)?;
                fs::create_dir_all(&misc_out)?;
                fs::write(misc_out.join("mp3nametable.json"), json)?;
            } else {
                // Sanitize: strip leading slashes so Path::join doesn't treat it as absolute.
                let safe_path = entry.path.trim_start_matches('/');
                let dst = misc_out.join(safe_path);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("creating dir for lookup {}", dst.display()))?;
                }
                fs::write(&dst, &data)
                    .with_context(|| format!("writing lookup {}", dst.display()))?;
            }
            if verbose {
                println!("lookup: {}", entry.path);
            }
            exported += 1;
        }
    }

    // Sounds (WAV files)
    {
        for &idx in &manifest.wav_entries {
            let rel_path = strip_data_prefix(&translated_paths[idx]);
            let Some(data) = read_entry(grf, idx) else {
                continue;
            };
            let dst = output.join(rel_path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dst, &data)?;
        }
        if !manifest.wav_entries.is_empty() {
            if verbose {
                println!("sound: {} wav files", manifest.wav_entries.len());
            }
            exported += 1;
        }
    }

    // Maps: texture/, model/, and map triads
    {
        // Texture entries.
        if !manifest.texture_entries.is_empty() {
            let tex_out = output.join("tex");
            for &idx in &manifest.texture_entries {
                let rel_path = strip_data_prefix(&translated_paths[idx]);
                // Strip "texture/" prefix since output is "tex/".
                let sub_path = rel_path.strip_prefix("texture/").unwrap_or(rel_path);
                let Some(data) = read_entry(grf, idx) else {
                    continue;
                };

                let filename = Path::new(sub_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let parent_rel = Path::new(sub_path).parent();

                // Apply translation if known map is available.
                let (final_dir, final_filename) = if let Some(k) = known {
                    let translated_name =
                        translate::translate_utf8_segment(&filename, k, &mut miss_log);
                    let dir = if let Some(p) = parent_rel {
                        let translated_dir = p
                            .components()
                            .map(|c| {
                                translate::translate_utf8_segment(
                                    &c.as_os_str().to_string_lossy(),
                                    k,
                                    &mut miss_log,
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        tex_out.join(translated_dir)
                    } else {
                        tex_out.clone()
                    };
                    (dir, translated_name)
                } else {
                    let dir = if let Some(p) = parent_rel {
                        tex_out.join(p)
                    } else {
                        tex_out.clone()
                    };
                    (dir, filename.clone())
                };

                fs::create_dir_all(&final_dir)?;

                if filename.to_ascii_lowercase().ends_with(".bmp") {
                    let png_name = bmp_to_png_name(&final_filename);
                    convert_bmp_to_png_from_data(&data, &final_dir.join(png_name))?;
                } else if filename.to_ascii_lowercase().ends_with(".str") {
                    let rewritten = ro_str::rewrite_textures(&data)
                        .with_context(|| format!("rewriting STR from GRF entry {idx}"))?;
                    fs::write(final_dir.join(&final_filename), rewritten)?;
                } else {
                    fs::write(final_dir.join(&final_filename), &data)?;
                }
            }
            if verbose {
                println!("map textures: {} entries", manifest.texture_entries.len());
            }
        }

        // Model entries.
        if !manifest.model_entries.is_empty() {
            let model_out = output.join("model");
            for &idx in &manifest.model_entries {
                let rel_path = strip_data_prefix(&translated_paths[idx]);
                let sub_path = rel_path.strip_prefix("model/").unwrap_or(rel_path);
                let Some(data) = read_entry(grf, idx) else {
                    continue;
                };

                let filename = Path::new(sub_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let parent_rel = Path::new(sub_path).parent();

                let (final_dir, final_filename) = if let Some(k) = known {
                    let translated_name =
                        translate::translate_utf8_segment(&filename, k, &mut miss_log);
                    let dir = if let Some(p) = parent_rel {
                        let translated_dir = p
                            .components()
                            .map(|c| {
                                translate::translate_utf8_segment(
                                    &c.as_os_str().to_string_lossy(),
                                    k,
                                    &mut miss_log,
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        model_out.join(translated_dir)
                    } else {
                        model_out.clone()
                    };
                    (dir, translated_name)
                } else {
                    let dir = if let Some(p) = parent_rel {
                        model_out.join(p)
                    } else {
                        model_out.clone()
                    };
                    (dir, filename.clone())
                };

                fs::create_dir_all(&final_dir)?;

                if filename.to_ascii_lowercase().ends_with(".bmp") {
                    let png_name = bmp_to_png_name(&final_filename);
                    convert_bmp_to_png_from_data(&data, &final_dir.join(png_name))?;
                } else if filename.to_ascii_lowercase().ends_with(".rsm") {
                    let rewritten = if let Some(k) = known {
                        rsm::rewrite_textures(&data, k, &mut miss_log)
                            .with_context(|| format!("rewriting RSM from GRF entry {idx}"))?
                    } else {
                        data
                    };
                    fs::write(final_dir.join(&final_filename), &rewritten)?;
                } else {
                    fs::write(final_dir.join(&final_filename), &data)?;
                }
            }
            if verbose {
                println!("map models: {} entries", manifest.model_entries.len());
            }
        }

        // Map triads (RSW/GND/GAT).
        let maps_dir = output.join("maps");
        for entry in &manifest.map {
            let rsw_data = read_entry(grf, entry.rsw_idx);
            let gnd_data = read_entry(grf, entry.gnd_idx);
            let gat_data = read_entry(grf, entry.gat_idx);
            let (Some(rsw_data), Some(gnd_data), Some(gat_data)) =
                (rsw_data, gnd_data, gat_data)
            else {
                skip_log.push_str(&format!(
                    "# SKIPPED: missing map file(s)\n[[map]]\nname = \"{}\"\n\n",
                    entry.name
                ));
                skipped += 1;
                continue;
            };
            let out_dir = maps_dir.join(&entry.name);
            fs::create_dir_all(&out_dir)?;
            if let Some(k) = known {
                let new_rsw = rsw::rewrite_model_paths(&rsw_data, k, &mut miss_log)
                    .with_context(|| format!("rewriting RSW for map {}", entry.name))?;
                let new_gnd = gnd::rewrite_textures(&gnd_data, k, &mut miss_log)
                    .with_context(|| format!("rewriting GND for map {}", entry.name))?;
                fs::write(out_dir.join(format!("{}.rsw", entry.name)), &new_rsw)?;
                fs::write(out_dir.join(format!("{}.gnd", entry.name)), &new_gnd)?;
            } else {
                fs::write(out_dir.join(format!("{}.rsw", entry.name)), &rsw_data)?;
                fs::write(out_dir.join(format!("{}.gnd", entry.name)), &gnd_data)?;
            }
            fs::write(out_dir.join(format!("{}.gat", entry.name)), &gat_data)?;
            if verbose {
                println!("map: {}", entry.name);
            }
            exported += 1;
        }
    }

    println!("Exported: {exported}  Skipped: {skipped}");

    if !skip_log.is_empty() {
        let skip_path = output.join("skipped.toml");
        fs::write(&skip_path, &skip_log)?;
        println!("Skip log: {}", skip_path.display());
    }

    if !miss_log.is_empty() {
        let miss_path = output.join("translation_misses.toml");
        let mut content = String::from(
            "# Translation misses: add entries to translations.toml [known] and re-run\n[known]\n",
        );
        for term in &miss_log {
            content.push_str(&format!("{term:?} = \"\"\n"));
        }
        fs::write(&miss_path, &content)?;
        println!(
            "Translation misses ({}): {}",
            miss_log.len(),
            miss_path.display()
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_entry<R: Read + Seek>(grf: &mut Grf<R>, idx: usize) -> Option<Vec<u8>> {
    let e = &grf.entries[idx];
    let snapshot = ro_files::GrfEntry {
        internal_path: e.internal_path.clone(),
        pack_size: e.pack_size,
        length_aligned: e.length_aligned,
        real_size: e.real_size,
        entry_type: e.entry_type,
        data_offset: e.data_offset,
    };
    grf.read_entry(&snapshot).ok()
}

fn strip_data_prefix(path: &str) -> &str {
    path.strip_prefix("data/").unwrap_or(path)
}

fn log_skip(log: &mut String, table: &str, entry_desc: &str, reason: &str) {
    log.push_str(&format!("# SKIPPED: {reason}\n[[{table}]]\n# {entry_desc}\n\n"));
}

fn bmp_to_png_name(name: &str) -> String {
    if name.to_ascii_lowercase().ends_with(".bmp") {
        format!("{}.png", &name[..name.len() - 4])
    } else {
        name.to_string()
    }
}

fn convert_bmp_to_png_from_data(data: &[u8], dst: &Path) -> Result<()> {
    let img = image::load_from_memory_with_format(data, image::ImageFormat::Bmp)
        .context("decoding BMP from GRF")?;
    let mut rgba_img = img.to_rgba8();

    for pixel in rgba_img.pixels_mut() {
        if pixel.0[0] == 255 && pixel.0[1] == 0 && pixel.0[2] == 255 {
            pixel.0[3] = 0;
        }
    }

    // Edge dilation: propagate opaque pixel colors into transparent border pixels.
    let (width, height) = rgba_img.dimensions();
    for _ in 0..2 {
        let reference = rgba_img.clone();
        for y in 0..height {
            for x in 0..width {
                if reference.get_pixel(x, y).0[3] != 0 {
                    continue;
                }
                let neighbors = [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ];
                let (mut r, mut g, mut b, mut count) = (0u32, 0u32, 0u32, 0u32);
                for (nx, ny) in neighbors {
                    if nx < width && ny < height {
                        let np = reference.get_pixel(nx, ny).0;
                        if np[3] != 0 {
                            r += np[0] as u32;
                            g += np[1] as u32;
                            b += np[2] as u32;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    let p = rgba_img.get_pixel_mut(x, y);
                    p.0[0] = (r / count) as u8;
                    p.0[1] = (g / count) as u8;
                    p.0[2] = (b / count) as u8;
                }
            }
        }
    }

    rgba_img
        .save(dst)
        .with_context(|| format!("saving PNG {}", dst.display()))?;
    Ok(())
}

/// Parse mp3nametable.txt from raw bytes (may contain CP949).
fn parse_mp3nametable(data: &[u8]) -> Result<HashMap<String, String>> {
    let text = String::from_utf8_lossy(data);
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '#').collect();
        if parts.len() < 2 {
            continue;
        }
        let rsw_part = parts[0].trim();
        let bgm_part = parts[1].trim();
        if rsw_part.is_empty() || bgm_part.is_empty() {
            continue;
        }
        let map_name = Path::new(&rsw_part.replace('\\', "/"))
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let bgm_path = bgm_part.replace('\\', "/").replace("//", "/");
        if !map_name.is_empty() && !bgm_path.is_empty() {
            map.insert(map_name, bgm_path);
        }
    }
    Ok(map)
}
