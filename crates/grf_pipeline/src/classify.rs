use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Manifest entry types (in-memory, not serialized)
// ---------------------------------------------------------------------------

pub struct Manifest {
    pub body: Vec<BodyEntry>,
    pub head: Vec<HeadEntry>,
    pub headgear: Vec<HeadgearEntry>,
    pub garment: Vec<GarmentEntry>,
    pub weapon: Vec<WeaponEntry>,
    pub shield: Vec<ShieldEntry>,
    pub shadow: Vec<ShadowEntry>,
    pub projectile: Vec<ProjectileEntry>,
    pub monster: Vec<MonsterEntry>,
    pub map: Vec<MapEntry>,
    pub effect: Vec<EffectEntry>,
    pub lookup: Vec<LookupEntry>,
    /// Indices of GRF entries under texture/ (for map export).
    pub texture_entries: Vec<usize>,
    /// Indices of GRF entries under model/ (for map export).
    pub model_entries: Vec<usize>,
    /// Indices of GRF entries under wav/ (for sound export).
    pub wav_entries: Vec<usize>,
}

pub struct BodyEntry {
    pub job: String,
    pub gender: String,
    /// Index into the GRF entries for the SPR file.
    pub spr_idx: usize,
    /// Index into the GRF entries for the ACT file.
    pub act_idx: usize,
    /// Optional index for the IMF file.
    pub imf_idx: Option<usize>,
}

pub struct HeadEntry {
    pub id: u32,
    pub gender: String,
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct HeadgearEntry {
    pub name: String,
    #[allow(dead_code)]
    pub view: u32,
    #[allow(dead_code)]
    pub slot: String,
    pub gender: String,
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct GarmentEntry {
    pub name: String,
    pub job: String,
    pub gender: String,
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct WeaponEntry {
    pub name: String,
    pub job: String,
    pub gender: String,
    pub slot: String,
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct ShieldEntry {
    pub name: String,
    pub job: String,
    pub gender: String,
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct ShadowEntry {
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct ProjectileEntry {
    pub name: String,
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct MonsterEntry {
    pub name: String,
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct MapEntry {
    pub name: String,
    pub rsw_idx: usize,
    pub gnd_idx: usize,
    pub gat_idx: usize,
}

pub struct EffectEntry {
    pub name: String,
    pub spr_idx: usize,
    pub act_idx: usize,
}

pub struct LookupEntry {
    pub path: String,
    pub idx: usize,
}

// ---------------------------------------------------------------------------
// headgear_slots.toml / weapon_types.toml deserialization
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct HeadgearSlotEntry {
    view: u32,
    slot: String,
    accname: Option<String>,
}

#[derive(serde::Deserialize)]
struct HeadgearSlotsFile {
    headgear: Vec<HeadgearSlotEntry>,
}

#[derive(serde::Deserialize)]
struct WeaponTypeEntry {
    name: String,
    items: Vec<u32>,
}

#[derive(serde::Deserialize)]
struct WeaponTypesFile {
    weapon_type: Vec<WeaponTypeEntry>,
}

// ---------------------------------------------------------------------------
// Classification config
// ---------------------------------------------------------------------------

pub struct ClassifyConfig {
    /// accname -> (view, slot) for headgear classification.
    pub accname_map: HashMap<String, (u32, String)>,
    /// item ID -> weapon type name for weapon classification.
    pub id_to_weapon_type: HashMap<u32, String>,
}

impl ClassifyConfig {
    pub fn load(
        slots_file: Option<&std::path::Path>,
        weapon_types_file: Option<&std::path::Path>,
        types: Option<&[String]>,
    ) -> anyhow::Result<Self> {
        let want = |t: &str| types.is_none() || types.is_some_and(|ts| ts.iter().any(|x| x == t));

        let accname_map = if want("headgear") {
            if let Some(path) = slots_file {
                let text = std::fs::read_to_string(path)
                    .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
                let data: HeadgearSlotsFile = toml::from_str(&text)
                    .map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;
                data.headgear
                    .iter()
                    .filter_map(|e| {
                        e.accname
                            .as_ref()
                            .map(|name| (name.clone(), (e.view, e.slot.clone())))
                    })
                    .collect()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        let id_to_weapon_type = if want("weapon") {
            if let Some(path) = weapon_types_file {
                let text = std::fs::read_to_string(path)
                    .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
                let data: WeaponTypesFile = toml::from_str(&text)
                    .map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;
                data.weapon_type
                    .iter()
                    .flat_map(|e| e.items.iter().map(|&id| (id, e.name.clone())))
                    .collect()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        Ok(Self {
            accname_map,
            id_to_weapon_type,
        })
    }
}

// ---------------------------------------------------------------------------
// Main classification entry point
// ---------------------------------------------------------------------------

/// Classify translated GRF paths into asset types.
///
/// `translated_paths` are forward-slash normalized paths (e.g. "data/sprite/human/body/male/novice_male.spr").
/// `entries` are the corresponding GRF entries (same length/order).
/// `act_action_counts` is called to read ACT action count from GRF for projectile classification.
pub fn classify(
    translated_paths: &[String],
    config: &ClassifyConfig,
    types: Option<&[String]>,
    act_action_count_fn: &mut dyn FnMut(usize) -> Option<u16>,
) -> Manifest {
    let want = |t: &str| types.is_none() || types.is_some_and(|ts| ts.iter().any(|x| x == t));

    // Build a path-to-index map for quick sibling lookups (e.g., find .act given .spr).
    let path_index: HashMap<&str, usize> = translated_paths
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_str(), i))
        .collect();

    let mut manifest = Manifest {
        body: Vec::new(),
        head: Vec::new(),
        headgear: Vec::new(),
        garment: Vec::new(),
        weapon: Vec::new(),
        shield: Vec::new(),
        shadow: Vec::new(),
        projectile: Vec::new(),
        monster: Vec::new(),
        map: Vec::new(),
        effect: Vec::new(),
        lookup: Vec::new(),
        texture_entries: Vec::new(),
        model_entries: Vec::new(),
        wav_entries: Vec::new(),
    };

    // Track which .spr stems we've seen to avoid duplicates.
    // Also collect sets for multi-pass classification.
    let mut rsw_stems: Vec<(String, usize)> = Vec::new();

    for (i, path) in translated_paths.iter().enumerate() {
        let normalized = strip_data_prefix(path);

        // Texture directory entries (for map export).
        if normalized.starts_with("texture/") && want("map") {
            manifest.texture_entries.push(i);
            continue;
        }

        // Model directory entries (for map export).
        if normalized.starts_with("model/") && want("map") {
            manifest.model_entries.push(i);
            continue;
        }

        // WAV directory entries (for sound export).
        if normalized.starts_with("wav/") && want("sound") {
            manifest.wav_entries.push(i);
            continue;
        }

        // Only process .spr files for sprite classification (the .act is looked up as sibling).
        // Exception: .rsw for maps, .txt for lookups.
        let ext = path_ext(normalized);

        if ext == "rsw" && want("map") {
            if let Some(stem) = path_stem(normalized) {
                rsw_stems.push((stem.to_string(), i));
            }
            continue;
        }

        if ext == "txt" && want("lookup") {
            manifest.lookup.push(LookupEntry {
                path: normalized.to_string(),
                idx: i,
            });
            continue;
        }

        if ext != "spr" {
            continue;
        }

        let spr_path = path.as_str();
        let act_path_str = format!("{}.act", &spr_path[..spr_path.len() - 4]);
        let Some(&act_idx) = path_index.get(act_path_str.as_str()) else {
            continue;
        };

        // Shadow
        if normalized == "sprite/shadow.spr" && want("shadow") {
            manifest.shadow.push(ShadowEntry {
                spr_idx: i,
                act_idx,
            });
            continue;
        }

        // Body sprites: sprite/human/body/{gender}/{stem}.spr
        if let Some(rest) = normalized.strip_prefix("sprite/human/body/") {
            if want("body") {
                classify_body(rest, i, act_idx, path, &path_index, &mut manifest);
            }
            continue;
        }

        // Head sprites: sprite/human/head/{gender}/{stem}.spr
        if let Some(rest) = normalized.strip_prefix("sprite/human/head/") {
            if want("head") {
                classify_head(rest, i, act_idx, &mut manifest);
            }
            continue;
        }

        // Headgear sprites: sprite/accessory/{gender}/{stem}.spr
        if let Some(rest) = normalized.strip_prefix("sprite/accessory/") {
            if want("headgear") {
                classify_headgear(rest, i, act_idx, &config.accname_map, &mut manifest);
            }
            continue;
        }

        // Garment sprites: sprite/robe/{name}/{gender}/{stem}.spr
        if let Some(rest) = normalized.strip_prefix("sprite/robe/") {
            if want("garment") {
                classify_garment(rest, i, act_idx, &mut manifest);
            }
            continue;
        }

        // Shield sprites: sprite/shield/{job}/{stem}.spr
        if let Some(rest) = normalized.strip_prefix("sprite/shield/") {
            if want("shield") {
                classify_shield(rest, i, act_idx, &mut manifest);
            }
            continue;
        }

        // Effect sprites: sprite/effect/{stem}.spr
        if let Some(rest) = normalized.strip_prefix("sprite/effect/") {
            if want("effect") {
                classify_effect(rest, i, act_idx, &mut manifest);
            }
            continue;
        }

        // Monster/projectile sprites: sprite/monster/{stem}.spr
        // Action count distinguishes them: 1 or 8 actions = projectile, otherwise = monster.
        if let Some(rest) = normalized.strip_prefix("sprite/monster/") {
            if want("monster") || want("projectile") {
                let action_count = act_action_count_fn(act_idx);
                let is_projectile = matches!(action_count, Some(1) | Some(8));
                if is_projectile && want("projectile") {
                    classify_projectile(rest, i, act_idx, act_action_count_fn, &mut manifest);
                } else if !is_projectile && want("monster") {
                    classify_monster(rest, i, act_idx, &mut manifest);
                }
            }
            continue;
        }

        // Weapon sprites: sprite/human/{job}/{stem}.spr (not body/head/accessory)
        if let Some(rest) = normalized.strip_prefix("sprite/human/") {
            if want("weapon") {
                classify_weapon(rest, i, act_idx, &config.id_to_weapon_type, &path_index, &mut manifest);
            }
            continue;
        }
    }

    // Maps: find RSW/GND/GAT triads.
    if want("map") {
        classify_maps(&rsw_stems, &path_index, translated_paths, &mut manifest);
    }

    // Sort for deterministic output.
    manifest.body.sort_by(|a, b| a.job.cmp(&b.job).then(a.gender.cmp(&b.gender)));
    manifest.head.sort_by(|a, b| a.gender.cmp(&b.gender).then(a.id.cmp(&b.id)));
    manifest.headgear.sort_by(|a, b| a.gender.cmp(&b.gender).then(a.name.cmp(&b.name)));
    manifest.garment.sort_by(|a, b| a.name.cmp(&b.name).then(a.gender.cmp(&b.gender)));
    manifest.weapon.sort_by(|a, b| a.job.cmp(&b.job).then(a.gender.cmp(&b.gender)).then(a.name.cmp(&b.name)));
    manifest.shield.sort_by(|a, b| a.job.cmp(&b.job).then(a.gender.cmp(&b.gender)).then(a.name.cmp(&b.name)));
    manifest.projectile.sort_by(|a, b| a.name.cmp(&b.name));
    manifest.monster.sort_by(|a, b| a.name.cmp(&b.name));
    manifest.map.sort_by(|a, b| a.name.cmp(&b.name));
    manifest.effect.sort_by(|a, b| a.name.cmp(&b.name));

    manifest
}

// ---------------------------------------------------------------------------
// Per-type classifiers
// ---------------------------------------------------------------------------

fn classify_body(
    rest: &str,
    spr_idx: usize,
    act_idx: usize,
    full_path: &str,
    path_index: &HashMap<&str, usize>,
    manifest: &mut Manifest,
) {
    // rest = "{gender}/{stem}.spr" or "{gender}/costume_{n}/{stem}.spr"
    let Some((gender, after_gender)) = rest.split_once('/') else {
        return;
    };
    if gender != "male" && gender != "female" {
        return;
    }

    // Check for costume subdirectory.
    if let Some((subdir, stem_spr)) = after_gender.split_once('/') {
        if let Some(n_str) = subdir.strip_prefix("costume_") {
            if n_str.parse::<u32>().is_err() {
                return;
            }
            let stem = strip_ext(stem_spr);
            let suffix = format!("_{gender}_{n_str}");
            let Some(job_base) = stem.strip_suffix(&suffix) else {
                return;
            };
            manifest.body.push(BodyEntry {
                job: format!("{job_base}_costume_{n_str}"),
                gender: gender.to_string(),
                spr_idx,
                act_idx,
                imf_idx: None,
            });
        }
        return;
    }

    let stem = strip_ext(after_gender);

    // Skip known artifacts.
    if stem.contains("_h_") || stem.contains("''") {
        return;
    }

    let opposite = if gender == "male" { "female" } else { "male" };
    let suffix = format!("_{gender}");
    let job = if let Some(j) = stem.strip_suffix(&suffix) {
        j.to_string()
    } else if stem == format!("dancer_{gender}_pants") {
        "dancer_costume_1".to_string()
    } else if !stem.contains(&format!("_{gender}")) && !stem.contains(&format!("_{opposite}")) {
        stem.to_string()
    } else {
        return;
    };

    // Look up IMF file.
    let imf_path = format!(
        "{}imf/{stem}.imf",
        data_prefix_from(full_path)
    );
    let imf_idx = path_index.get(imf_path.as_str()).copied();

    manifest.body.push(BodyEntry {
        job,
        gender: gender.to_string(),
        spr_idx,
        act_idx,
        imf_idx,
    });
}

fn classify_head(rest: &str, spr_idx: usize, act_idx: usize, manifest: &mut Manifest) {
    // rest = "{gender}/{id}_{gender}.spr"
    let Some((gender, stem_spr)) = rest.split_once('/') else {
        return;
    };
    if gender != "male" && gender != "female" {
        return;
    }
    let stem = strip_ext(stem_spr);
    let suffix = format!("_{gender}");
    let Some(id_str) = stem.strip_suffix(&suffix) else {
        return;
    };
    let Ok(id) = id_str.parse::<u32>() else {
        return;
    };
    manifest.head.push(HeadEntry {
        id,
        gender: gender.to_string(),
        spr_idx,
        act_idx,
    });
}

fn classify_headgear(
    rest: &str,
    spr_idx: usize,
    act_idx: usize,
    accname_map: &HashMap<String, (u32, String)>,
    manifest: &mut Manifest,
) {
    // rest = "{gender}/{prefix}_{accname}.spr"
    let Some((gender, stem_spr)) = rest.split_once('/') else {
        return;
    };
    if gender != "male" && gender != "female" {
        return;
    }
    let stem = strip_ext(stem_spr);
    let prefix = format!("{gender}_");
    let Some(accname) = stem.strip_prefix(&prefix) else {
        return;
    };
    let (view, slot) = accname_map
        .get(accname)
        .map(|(v, s)| (*v, s.clone()))
        .unwrap_or((0, "Head_Top".to_string()));
    manifest.headgear.push(HeadgearEntry {
        name: accname.to_string(),
        view,
        slot,
        gender: gender.to_string(),
        spr_idx,
        act_idx,
    });
}

fn classify_garment(rest: &str, spr_idx: usize, act_idx: usize, manifest: &mut Manifest) {
    // rest = "{garment_name}/{gender}/{stem}.spr"
    let parts: Vec<&str> = rest.splitn(3, '/').collect();
    if parts.len() != 3 {
        return;
    }
    let garment_name = parts[0];
    let gender = parts[1];
    let stem_spr = parts[2];
    if gender != "male" && gender != "female" {
        return;
    }
    let stem = strip_ext(stem_spr);
    let suffix = format!("_{gender}");
    let Some(job) = stem.strip_suffix(&suffix) else {
        return;
    };
    manifest.garment.push(GarmentEntry {
        name: garment_name.to_string(),
        job: job.to_string(),
        gender: gender.to_string(),
        spr_idx,
        act_idx,
    });
}

fn classify_weapon(
    rest: &str,
    spr_idx: usize,
    act_idx: usize,
    id_to_weapon_type: &HashMap<u32, String>,
    path_index: &HashMap<&str, usize>,
    manifest: &mut Manifest,
) {
    // rest = "{job_dir}/{stem}.spr"
    let Some((job_dir, stem_spr)) = rest.split_once('/') else {
        return;
    };
    // Skip body/head directories (already handled).
    if job_dir == "body" || job_dir == "head" {
        return;
    }
    // Skip if stem_spr contains another '/' (subdirectory like accessory/male/...).
    if stem_spr.contains('/') {
        return;
    }
    let stem = strip_ext(stem_spr);

    // Mercenary weapons.
    if job_dir == "mercenary" {
        let (weapon_stem, slot) = if let Some(base) = stem.strip_suffix("_slash_glow") {
            (base, "slash")
        } else {
            (stem, "weapon")
        };
        let Some(merc_pos) = weapon_stem.find("_mercenary_") else {
            return;
        };
        let job_prefix = &weapon_stem[..merc_pos];
        let weapon_name = &weapon_stem[merc_pos + "_mercenary_".len()..];
        let job = format!("{job_prefix}_mercenary");
        for gender in ["male", "female"] {
            manifest.weapon.push(WeaponEntry {
                name: weapon_name.to_string(),
                job: job.clone(),
                gender: gender.to_string(),
                slot: slot.to_string(),
                spr_idx,
                act_idx,
            });
        }
        return;
    }

    const SHIELD_NAMES: &[&str] = &["guard", "buckler_", "mirror_shield_", "te_woe_shield"];

    let prefix = format!("{job_dir}_");
    let Some(after_prefix) = stem.strip_prefix(&prefix) else {
        return;
    };
    let Some(us) = after_prefix.find('_') else {
        return;
    };
    let gender = &after_prefix[..us];
    if gender != "male" && gender != "female" {
        return;
    }
    let weapon_part = &after_prefix[us + 1..];
    let base_weapon = weapon_part
        .strip_suffix("_slash_glow")
        .unwrap_or(weapon_part);
    if SHIELD_NAMES.contains(&base_weapon) {
        return;
    }
    let (weapon_name, slot) = if let Some(base) = weapon_part.strip_suffix("_slash_glow") {
        (base.to_string(), "slash".to_string())
    } else {
        (weapon_part.to_string(), "weapon".to_string())
    };

    // Skip ID-based weapon sprites.
    if let Ok(item_id) = weapon_name.parse::<u32>() {
        let type_hint = id_to_weapon_type
            .get(&item_id)
            .map(|t| format!(" (type: {t})"))
            .unwrap_or_default();
        eprintln!("warning: skipping ID-based weapon sprite '{stem}'{type_hint}");
        return;
    }

    // swordsman_female_two_handed_sword.act workaround.
    let final_act_idx = if job_dir == "swordsman"
        && gender == "female"
        && weapon_name == "two_handed_sword"
        && slot == "weapon"
    {
        let alt_act = format!(
            "{}sprite/human/swordsman/swordsman_female_sword.act",
            data_prefix_from_rest("sprite/human/", rest)
        );
        path_index.get(alt_act.as_str()).copied().unwrap_or(act_idx)
    } else {
        act_idx
    };

    manifest.weapon.push(WeaponEntry {
        name: weapon_name,
        job: job_dir.to_string(),
        gender: gender.to_string(),
        slot,
        spr_idx,
        act_idx: final_act_idx,
    });
}

fn classify_shield(rest: &str, spr_idx: usize, act_idx: usize, manifest: &mut Manifest) {
    // rest = "{job}/{stem}.spr"
    let Some((job, stem_spr)) = rest.split_once('/') else {
        return;
    };
    if stem_spr.contains('/') {
        return;
    }
    let stem = strip_ext(stem_spr);
    let prefix = format!("{job}_");
    let Some(after_prefix) = stem.strip_prefix(&prefix) else {
        return;
    };
    let Some(us) = after_prefix.find('_') else {
        return;
    };
    let gender = &after_prefix[..us];
    if gender != "male" && gender != "female" {
        return;
    }
    let raw_name = &after_prefix[us + 1..];

    // Skip ID-based shields.
    if raw_name
        .strip_suffix("_shield")
        .is_some_and(|id| id.parse::<u32>().is_ok())
    {
        eprintln!("warning: skipping ID-based shield sprite '{stem}'");
        return;
    }

    let shield_name = if raw_name == "te_woe_shield" {
        "shield".to_string()
    } else {
        raw_name.to_string()
    };

    manifest.shield.push(ShieldEntry {
        name: shield_name,
        job: job.to_string(),
        gender: gender.to_string(),
        spr_idx,
        act_idx,
    });
}

fn classify_effect(rest: &str, spr_idx: usize, act_idx: usize, manifest: &mut Manifest) {
    // rest = "{stem}.spr" (flat directory)
    if rest.contains('/') {
        return;
    }
    let stem = strip_ext(rest);
    manifest.effect.push(EffectEntry {
        name: stem.to_string(),
        spr_idx,
        act_idx,
    });
}

fn classify_monster(rest: &str, spr_idx: usize, act_idx: usize, manifest: &mut Manifest) {
    // rest = "{stem}.spr" (flat directory)
    if rest.contains('/') {
        return;
    }
    let stem = strip_ext(rest);
    manifest.monster.push(MonsterEntry {
        name: stem.to_string(),
        spr_idx,
        act_idx,
    });
}

fn classify_projectile(
    rest: &str,
    spr_idx: usize,
    act_idx: usize,
    act_action_count_fn: &mut dyn FnMut(usize) -> Option<u16>,
    manifest: &mut Manifest,
) {
    // rest = "{stem}.spr" (flat directory)
    if rest.contains('/') {
        return;
    }
    let stem = strip_ext(rest);

    // Only include sprites with 1 or 8 actions (projectile heuristic).
    let action_count = match act_action_count_fn(act_idx) {
        Some(n) => n,
        None => return,
    };
    if action_count != 1 && action_count != 8 {
        return;
    }

    let name_overrides: HashMap<&str, &str> = [("skel_archer_arrow", "arrow")].into_iter().collect();
    let name = name_overrides
        .get(stem)
        .copied()
        .unwrap_or(stem);

    manifest.projectile.push(ProjectileEntry {
        name: name.to_string(),
        spr_idx,
        act_idx,
    });
}

fn classify_maps(
    rsw_stems: &[(String, usize)],
    path_index: &HashMap<&str, usize>,
    translated_paths: &[String],
    manifest: &mut Manifest,
) {
    // Map triads: find matching .gnd and .gat for each .rsw.
    // RSW paths are at the data root level: "data/{stem}.rsw" -> normalized stem from path.
    for (stem, rsw_idx) in rsw_stems {
        let rsw_path: &str = &translated_paths[*rsw_idx];
        let base = &rsw_path[..rsw_path.len() - 4]; // strip ".rsw"
        let gnd_path = format!("{base}.gnd");
        let gat_path = format!("{base}.gat");
        let gnd_idx = path_index.get(gnd_path.as_str()).copied();
        let gat_idx = path_index.get(gat_path.as_str()).copied();
        match (gnd_idx, gat_idx) {
            (Some(gnd), Some(gat)) => {
                manifest.map.push(MapEntry {
                    name: stem.clone(),
                    rsw_idx: *rsw_idx,
                    gnd_idx: gnd,
                    gat_idx: gat,
                });
            }
            _ => {
                eprintln!(
                    "WARN: incomplete map triad for '{stem}' (missing .gnd or .gat), skipping"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Strip `data/` prefix from a translated path.
fn strip_data_prefix(path: &str) -> &str {
    path.strip_prefix("data/").unwrap_or(path)
}

/// Get file extension (lowercase) from a path.
fn path_ext(path: &str) -> &str {
    path.rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("")
}

/// Get the filename stem (without directory or extension) from a path.
fn path_stem(path: &str) -> Option<&str> {
    let filename = path.rsplit_once('/').map(|(_, f)| f).unwrap_or(path);
    filename.rsplit_once('.').map(|(s, _)| s)
}

/// Strip extension from a filename.
fn strip_ext(filename: &str) -> &str {
    filename.rsplit_once('.').map(|(s, _)| s).unwrap_or(filename)
}

/// Extract the "data/" prefix portion from a full translated path.
/// Returns "data/" if the path starts with "data/", else "".
fn data_prefix_from(path: &str) -> &str {
    if path.starts_with("data/") { "data/" } else { "" }
}

/// Build a data prefix for sibling lookup given a path suffix after "sprite/human/".
fn data_prefix_from_rest(_prefix: &str, _rest: &str) -> String {
    // Since all GRF paths use "data/" prefix consistently, just return "data/".
    "data/".to_string()
}

/// Print manifest summary.
pub fn print_summary(manifest: &Manifest) {
    println!(
        "  bodies={} heads={} headgears={} garments={} weapons={} shields={} shadow={} projectiles={} maps={} effects={} lookups={} monsters={}",
        manifest.body.len(),
        manifest.head.len(),
        manifest.headgear.len(),
        manifest.garment.len(),
        manifest.weapon.len(),
        manifest.shield.len(),
        manifest.shadow.len(),
        manifest.projectile.len(),
        manifest.map.len(),
        manifest.effect.len(),
        manifest.lookup.len(),
        manifest.monster.len()
    );
    if !manifest.texture_entries.is_empty() {
        println!("  texture entries={}", manifest.texture_entries.len());
    }
    if !manifest.model_entries.is_empty() {
        println!("  model entries={}", manifest.model_entries.len());
    }
    if !manifest.wav_entries.is_empty() {
        println!("  wav entries={}", manifest.wav_entries.len());
    }
}
