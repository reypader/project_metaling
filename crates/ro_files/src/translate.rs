use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Deserialize, Serialize)]
pub struct TranslationsFile {
    #[serde(default)]
    pub known: HashMap<String, String>,
}

/// Strip a leading `data/` path component if present.
/// e.g. `data/texture/foo.bmp` → `texture/foo.bmp`, `data/model/foo.rsm` → `model/foo.rsm`
pub fn strip_data_prefix(path: &str) -> &str {
    let data_stripped = path.strip_prefix("data/").unwrap_or(path);
    let texture_stripped = data_stripped
        .strip_prefix("texture/")
        .unwrap_or(data_stripped);

    (texture_stripped
        .strip_prefix("texture\\")
        .unwrap_or(texture_stripped)) as _
}

pub fn load_known(path: &Path) -> Result<HashMap<String, String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading translations file {}", path.display()))?;
    let file: TranslationsFile = toml::from_str(&text)
        .with_context(|| format!("parsing translations file {}", path.display()))?;
    Ok(file.known)
}

/// Decode a CP949-encoded binary path string to a UTF-8 forward-slash path without translation.
/// Used for paths whose segments must be preserved as-is (e.g. WAV filenames in RSW).
pub fn decode_cp949_path(raw: &[u8]) -> String {
    let trimmed = raw.split(|&b| b == 0).next().unwrap_or(raw);
    let (decoded, _, _) = encoding_rs::EUC_KR.decode(trimmed);
    decoded.replace('\\', "/")
}

/// Translate a CP949-encoded, backslash-separated binary path string to a UTF-8 forward-slash path.
/// Used for path strings read directly from GND/RSW binary slots.
/// Any non-ASCII tokens that could not be resolved are inserted into `misses`.
pub fn translate_cp949_path(
    raw: &[u8],
    known: &HashMap<String, String>,
    misses: &mut BTreeSet<String>,
) -> String {
    let trimmed = raw.split(|&b| b == 0).next().unwrap_or(raw);
    let (decoded, _, _) = encoding_rs::EUC_KR.decode(trimmed);
    decoded
        .split('\\')
        .map(|seg| translate_segment(seg, known, misses))
        .collect::<Vec<_>>()
        .join("/")
}

/// Translate a single UTF-8 segment (already decoded; directory name or filename).
/// Used when walking the filesystem for translation-aware directory copies.
/// Any non-ASCII tokens that could not be resolved are inserted into `misses`.
pub fn translate_utf8_segment(
    seg: &str,
    known: &HashMap<String, String>,
    misses: &mut BTreeSet<String>,
) -> String {
    let (base, ext) = split_ext(seg);
    let translated_base = translate_segment(base, known, misses);
    if ext.is_empty() {
        translated_base
    } else {
        format!("{translated_base}{ext}")
    }
}

fn translate_segment(
    seg: &str,
    known: &HashMap<String, String>,
    misses: &mut BTreeSet<String>,
) -> String {
    if seg.is_ascii() {
        return seg.to_string();
    }
    if let Some(v) = known.get(seg) {
        return format!("e_{v}");
    }
    let (base, ext) = split_ext(seg);
    if !ext.is_empty()
        && let Some(v) = known.get(base)
    {
        return format!("e_{v}{ext}");
    }
    let tokens: Vec<String> = base
        .split('_')
        .map(|tok| {
            if tok.is_ascii() {
                return tok.to_string();
            }
            if let Some(v) = known.get(tok) {
                v.clone()
            } else {
                misses.insert(tok.to_string());
                tok.to_string()
            }
        })
        .collect();
    format!("e_{}{}", tokens.join("_"), ext)
}

/// Replace a `.bmp` (case-insensitive) extension with `.png`.
/// Used to normalize texture paths emitted into binary map files and output directories.
pub fn bmp_ext_to_png(path: &str) -> String {
    if path.to_ascii_lowercase().ends_with(".bmp") {
        format!("{}.png", &path[..path.len() - 4])
    } else {
        path.to_string()
    }
}

fn split_ext(name: &str) -> (&str, &str) {
    if let Some(dot) = name.rfind('.') {
        (&name[..dot], &name[dot..])
    } else {
        (name, "")
    }
}

// ---------------------------------------------------------------------------
// Translator
// ---------------------------------------------------------------------------

pub struct Translator {
    known: HashMap<String, String>,
    /// rAthena-derived lookup: Korean resource name → AegisName (lowercase).
    rathena: HashMap<String, String>,
    misses: BTreeSet<String>,
}

impl Translator {
    pub fn new(known: HashMap<String, String>, rathena: HashMap<String, String>) -> Self {
        Self {
            known,
            rathena,
            misses: BTreeSet::new(),
        }
    }

    /// Translate a full GRF internal path (backslash-separated, CP949-decoded).
    /// Returns the translated path using forward slashes.
    /// `add_e_prefix`: prepend `e_` to non-ASCII translated segments.
    pub fn translate_path(&mut self, grf_path: &str, add_e_prefix: bool) -> String {
        grf_path
            .split('\\')
            .map(|seg| self.translate_segment(seg, add_e_prefix))
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Translate a single path segment (directory name or filename).
    ///
    /// Strategy:
    /// 1. Pure ASCII → keep as-is.
    /// 2. Whole segment in `known` → use mapped value.
    /// 3. Whole segment in `rathena` → use AegisName.
    /// 4. Split on `_`, apply steps 1-3 per token; log misses.
    /// 5. If `add_e_prefix` and segment was non-ASCII, prepend `e_` to the base name.
    fn translate_segment(&mut self, segment: &str, add_e_prefix: bool) -> String {
        if segment.is_ascii() {
            return segment.to_string();
        }

        let pfx = if add_e_prefix { "e_" } else { "" };

        // Try whole segment.
        if let Some(english) = self.lookup(segment) {
            return format!("{pfx}{english}");
        }

        // Split extension (e.g. ".spr", ".act").
        let (base, ext) = split_ext(segment);

        // Try whole base without extension.
        if !ext.is_empty()
            && let Some(english) = self.lookup(base)
        {
            return format!("{pfx}{english}{ext}");
        }

        // Token-by-token translation on `_` boundaries.
        let tokens: Vec<String> = base
            .split('_')
            .map(|token| {
                if token.is_ascii() {
                    return token.to_string();
                }
                if let Some(english) = self.lookup(token) {
                    return english;
                }
                // Miss: log and keep original.
                self.misses.insert(token.to_string());
                token.to_string()
            })
            .collect();

        format!("{pfx}{}{}", tokens.join("_"), ext)
    }

    fn lookup(&self, key: &str) -> Option<String> {
        self.known
            .get(key)
            .or_else(|| self.rathena.get(key))
            .cloned()
    }

    /// Returns all Korean segments that could not be translated.
    pub fn misses(&self) -> &BTreeSet<String> {
        &self.misses
    }
}

// ---------------------------------------------------------------------------
// Miss log serialization
// ---------------------------------------------------------------------------

/// Serialize misses to a TOML snippet the user can fill in and merge into
/// translations.toml.
pub fn format_miss_log(misses: &BTreeSet<String>) -> String {
    if misses.is_empty() {
        return String::new();
    }

    let mut log = String::from(
        "# Translation misses — fill in the English values and move entries to translations.toml\n\n\
         [known]\n",
    );

    // Use BTreeMap so output is sorted.
    let map: BTreeMap<&str, &str> = misses.iter().map(|k| (k.as_str(), "")).collect();
    for k in map.keys() {
        log.push_str(&format!("{} = \"\"\n", toml_key(k)));
    }
    log
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Produce a TOML-safe key string (quote if it contains special characters).
fn toml_key(s: &str) -> String {
    // TOML bare keys allow ASCII alphanumeric, `-`, and `_` only.
    // Korean always needs quoting.
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
