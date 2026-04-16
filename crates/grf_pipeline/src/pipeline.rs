use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use ro_files::translate::{format_miss_log, TranslationsFile, Translator};
use ro_files::{Grf, GrfEntry};

use crate::{classify, export, headgear_slots, rathena, weapon_types};

const ITEM_RES_TABLE_PATH: &str = "data\\idnum2itemresnametable.txt";

/// Path prefixes that should NOT get the `e_` prefix on translated segments.
/// These are sprite-related paths that get renamed into a structured layout
/// during export, so filename collisions are not a concern.
const NO_E_PREFIX_PREFIXES: &[&str] = &["data/sprite/", "data/imf/", "data/palette/"];

#[allow(clippy::too_many_arguments)]
pub fn run(
    grf_path: &Path,
    output: &Path,
    translations_path: &Path,
    rathena_db: Option<&Path>,
    headgear_slots_path: &Path,
    weapon_types_path: &Path,
    types: Option<&[String]>,
    miss_log_path: &Path,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    // 1. Load translation dictionary.
    let known = load_known(translations_path)?;

    // 2. Open GRF.
    let file = fs::File::open(grf_path)
        .with_context(|| format!("opening {}", grf_path.display()))?;
    let mut grf = Grf::open(file)
        .with_context(|| format!("parsing {}", grf_path.display()))?;

    println!("GRF: {} entries", grf.entries.len());

    // 3. Build rAthena lookup (optional).
    let rathena_lookup = build_rathena_lookup(&mut grf, rathena_db)?;
    if !rathena_lookup.is_empty() {
        println!(
            "rAthena: {} item res name mappings loaded",
            rathena_lookup.len()
        );
    }

    // 4. Generate headgear_slots.toml and weapon_types.toml when rAthena DB is available.
    if let Some(db_path) = rathena_db {
        generate_headgear_slots(db_path, headgear_slots_path)?;
        generate_weapon_types(db_path, weapon_types_path)?;
    }

    // 5. Translate all GRF paths.
    let translated_paths: Vec<String> = {
        let mut t = Translator::new(known.clone(), rathena_lookup);
        let paths: Vec<String> = grf
            .entries
            .iter()
            .map(|e| {
                let normalized = e.internal_path.replace('\\', "/");
                let lower = normalized.to_ascii_lowercase();

                // Passthrough (no translation): lookup tables, effect assets.
                if lower.ends_with(".txt") || lower.ends_with(".tga") || lower.ends_with(".str")
                {
                    return normalized;
                }

                if normalized.is_ascii() {
                    return normalized;
                }

                // Sprite/IMF/palette paths: translate without e_ prefix.
                // These get renamed into a structured layout during export,
                // so collisions are not a concern.
                let no_prefix = NO_E_PREFIX_PREFIXES
                    .iter()
                    .any(|p| lower.starts_with(p));

                t.translate_path(&e.internal_path, !no_prefix)
            })
            .collect();

        // Write miss log.
        let miss_log_content = format_miss_log(t.misses());
        if !miss_log_content.is_empty() {
            fs::write(miss_log_path, &miss_log_content)
                .with_context(|| format!("writing miss log {}", miss_log_path.display()))?;
            println!(
                "Miss log: {} ({} untranslated segments)",
                miss_log_path.display(),
                t.misses().len()
            );
        }

        paths
    };

    // 6. Classify entries in-memory.
    let config = classify::ClassifyConfig::load(
        Some(headgear_slots_path),
        Some(weapon_types_path),
        types,
    )?;

    // Provide a callback to read ACT action counts from the GRF (for projectile classification).
    // We need to snapshot entry metadata to avoid borrow conflicts.
    let entry_snapshots: Vec<(u32, u32, u32, u8, u64, String)> = grf
        .entries
        .iter()
        .map(|e| {
            (
                e.pack_size,
                e.length_aligned,
                e.real_size,
                e.entry_type,
                e.data_offset,
                e.internal_path.clone(),
            )
        })
        .collect();

    let manifest = classify::classify(
        &translated_paths,
        &config,
        types,
        &mut |act_idx: usize| {
            let (pack_size, length_aligned, real_size, entry_type, data_offset, ref internal_path) =
                entry_snapshots[act_idx];
            let snapshot = GrfEntry {
                internal_path: internal_path.clone(),
                pack_size,
                length_aligned,
                real_size,
                entry_type,
                data_offset,
            };
            let data = grf.read_entry(&snapshot).ok()?;
            // ACT header: 2-byte magic "AC", 2-byte version, 2-byte action count.
            if data.len() < 6 || &data[..2] != b"AC" {
                return None;
            }
            Some(u16::from_le_bytes([data[4], data[5]]))
        },
    );

    classify::print_summary(&manifest);

    if dry_run {
        println!("Dry run: no files written.");
        return Ok(());
    }

    // 7. Export.
    let known_map = if known.is_empty() { None } else { Some(known) };

    export::export(
        &mut grf,
        &translated_paths,
        &manifest,
        output,
        &known_map,
        verbose,
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_known(path: &Path) -> Result<HashMap<String, String>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading translations file {}", path.display()))?;
    let file: TranslationsFile = toml::from_str(&text)
        .with_context(|| format!("parsing translations file {}", path.display()))?;
    Ok(file.known)
}

fn generate_headgear_slots(rathena_db: &Path, out_path: &Path) -> Result<()> {
    let equip_db = rathena_db.join("re/item_db_equip.yml");
    let headgear_map = headgear_slots::parse_headgear_items(&equip_db);
    let entries = headgear_slots::build_headgear_slots(&headgear_map);
    let count = entries.len();
    headgear_slots::write_headgear_slots(entries, out_path)
        .with_context(|| format!("writing {}", out_path.display()))?;
    println!("Headgear slots: {} entries -> {}", count, out_path.display());
    Ok(())
}

fn generate_weapon_types(rathena_db: &Path, out_path: &Path) -> Result<()> {
    let equip_db = rathena_db.join("re/item_db_equip.yml");
    let weapon_map = weapon_types::parse_weapon_items(&equip_db);
    let entries = weapon_types::build_weapon_types(weapon_map);
    let count = entries.len();
    weapon_types::write_weapon_types(entries, out_path)
        .with_context(|| format!("writing {}", out_path.display()))?;
    println!("Weapon types: {} entries -> {}", count, out_path.display());
    Ok(())
}

fn build_rathena_lookup(
    grf: &mut Grf<fs::File>,
    rathena_db: Option<&Path>,
) -> Result<HashMap<String, String>> {
    let table_entry = grf
        .entries
        .iter()
        .find(|e| e.internal_path.eq_ignore_ascii_case(ITEM_RES_TABLE_PATH))
        .map(|e| GrfEntry {
            internal_path: e.internal_path.clone(),
            pack_size: e.pack_size,
            length_aligned: e.length_aligned,
            real_size: e.real_size,
            entry_type: e.entry_type,
            data_offset: e.data_offset,
        });

    let Some(table_entry) = table_entry else {
        if rathena_db.is_some() {
            eprintln!("WARN: {ITEM_RES_TABLE_PATH} not found in GRF; skipping rAthena lookup");
        }
        return Ok(HashMap::new());
    };

    let table_data = grf
        .read_entry(&table_entry)
        .context("reading idnum2itemresnametable.txt from GRF")?;

    let res_table = rathena::parse_item_res_table(&table_data);
    println!("GRF item res table: {} entries", res_table.len());

    let Some(db_path) = rathena_db else {
        return Ok(HashMap::new());
    };

    let db_files = [
        "re/item_db_equip.yml",
        "re/item_db_usable.yml",
        "re/item_db_etc.yml",
        "pre-re/item_db_equip.yml",
    ];

    let rathena_dbs: Vec<HashMap<u32, String>> = db_files
        .iter()
        .map(|f| rathena::parse_rathena_item_db(&db_path.join(f)))
        .filter(|m| !m.is_empty())
        .collect();

    Ok(rathena::build_res_to_aegis(&res_table, &rathena_dbs))
}
