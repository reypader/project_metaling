#!/usr/bin/env python3
"""
Scan config/EffectTable.json for EUC-KR hex escape sequences (\\xNN...),
decode them as EUC-KR, look up the Korean text in config/translations.toml,
and rewrite the file with English replacements.

Run from the repository root:
    python util/decode_effect_table.py [--dry-run]

Hex escape sequences that cannot be matched in translations.toml are left
unchanged and reported on stderr as misses for manual follow-up.
"""

import argparse
import re
import sys
import tomllib

# Matches a JSON5 path segment that contains at least one \\xNN escape.
# Characters up to the nearest /, ", or ' boundary are included so that
# ASCII prefixes/suffixes (e.g. "mon_" in "mon_\\xNN...") are captured
# alongside the hex runs and the e_ prefix lands at the segment front.
SEGMENT_WITH_HEX_RE = re.compile(r"""[^/"']*?(?:\\x[0-9a-fA-F]{2})+[^/"']*""")

# Matches one or more consecutive \\xNN sequences within a segment.
HEX_RUN_RE = re.compile(r"(?:\\x[0-9a-fA-F]{2})+")


def load_translations(path: str) -> dict[str, str]:
    with open(path, "rb") as f:
        data = tomllib.load(f)
    return {k: v for k, v in data.get("known", {}).items() if v}


def decode_hex_run(run: str) -> bytes:
    """Convert a literal \\xNN\\xNN... run to raw bytes."""
    return bytes(int(h, 16) for h in re.findall(r"[0-9a-fA-F]{2}", run))


def decode_segment_hex(segment: str) -> str:
    """Replace every \\xNN run in a segment with its EUC-KR decoded text."""
    def decode_run(m: re.Match) -> str:
        try:
            return decode_hex_run(m.group(0)).decode("euc-kr")
        except (ValueError, UnicodeDecodeError):
            return m.group(0)
    return HEX_RUN_RE.sub(decode_run, segment)


def split_ext(name: str) -> tuple[str, str]:
    dot = name.rfind(".")
    if dot == -1:
        return name, ""
    return name[:dot], name[dot:]


def translate_segment(seg: str, known: dict[str, str], misses: list[str]) -> str:
    """
    Mirror the Rust `translate_segment` logic from `ro_files/src/translate.rs`:
      1. ASCII segments are returned unchanged.
      2. Whole-segment match in `known` -> `e_{translation}`.
      3. Base (extension stripped) match in `known` -> `e_{translation}{ext}`.
      4. Split on `_`, translate each non-ASCII token; unknown tokens are kept
         as-is and logged as misses. Result is joined with `_` and prefixed `e_`.
    """
    if seg.isascii():
        return seg

    if seg in known:
        return f"e_{known[seg]}"

    base, ext = split_ext(seg)
    if ext and base in known:
        return f"e_{known[base]}{ext}"

    tokens = []
    for tok in base.split("_"):
        if tok.isascii():
            tokens.append(tok)
        elif tok in known:
            tokens.append(known[tok])
        else:
            misses.append(tok)
            tokens.append(tok)

    return f"e_{'_'.join(tokens)}{ext}"


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--effect-table",
        default="../../config/EffectTable.json",
        help="Path to EffectTable.json (default: config/EffectTable.json)",
    )
    parser.add_argument(
        "--translations",
        default="../../config/translations.toml",
        help="Path to translations.toml (default: config/translations.toml)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the rewritten file to stdout instead of writing it",
    )
    args = parser.parse_args()

    known = load_translations(args.translations)

    with open(args.effect_table, encoding="utf-8") as f:
        text = f.read()

    misses: list[str] = []
    replacements = 0

    def replace_match(m: re.Match) -> str:
        nonlocal replacements

        raw = m.group(0)
        decoded = decode_segment_hex(raw)
        result = translate_segment(decoded, known, misses)
        if result != raw:
            replacements += 1
        return result

    new_text = SEGMENT_WITH_HEX_RE.sub(replace_match, text)

    if misses:
        print(
            f"[decode_effect_table] {len(misses)} untranslatable segment(s) "
            f"(add to config/translations.toml):",
            file=sys.stderr,
        )
        for k in sorted(set(misses)):
            print(f"  {k!r}", file=sys.stderr)

    print(
        f"[decode_effect_table] {replacements} replacement(s), "
        f"{len(misses)} miss(es)"
    )

    if args.dry_run:
        print(new_text)
        return

    with open(args.effect_table, "w", encoding="utf-8") as f:
        f.write(new_text)

    print(f"[decode_effect_table] Written to {args.effect_table}")


if __name__ == "__main__":
    main()
