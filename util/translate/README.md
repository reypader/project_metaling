### Translation Script

A helper script that reads `miss_log.toml` and uses the Claude API to
batch-translate untranslated Korean tokens, then appends the results to
`translations.toml` automatically. Useful when `miss_log.toml` grows large
after extracting a new GRF with many unfamiliar item names.

**Setup** (run once from the `util/` directory):

```sh
pyenv exec python -m venv .venv
.venv/bin/pip install -r requirements.txt
```

**Environment:**

```sh
export ANTHROPIC_API_KEY=sk-ant-...
```

**Basic usage** (run from `util/translate/`, paths default to `../../target/miss_log.toml` and
`../../config/translations.toml`):

```sh
.venv/bin/python translate.py
```

**Options:**

```
--model sonnet|haiku     Model to use (default: sonnet)
--batch-size N           Keys per API call (default: 200)
--miss-log PATH          Path to miss_log.toml (default: ../../target/miss_log.toml)
--translations PATH      Path to translations.toml (default: ../../config/translations.toml)
--progress PATH          Progress file for resume support (default: ./../../target/translation_progress.json)
--dry-run                Show batch plan without making API calls
--no-append              Translate and save progress only; skip writing translations.toml
-v, --verbose            Print per-batch token usage
--debug                  Print raw API response for the first batch
```

A `translation_progress.json` file is written after each batch so an interrupted
run can be resumed. Re-running the script skips keys already present in
`translations.toml` or the progress file. Once complete, re-run the extractor
with the updated `translations.toml` to resolve the previously missed paths.