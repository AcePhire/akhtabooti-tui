# akhtabooti-tui

A terminal UI for scanning files and directories for PIIs, built on [akhtabooti-core](https://crates.io/crates/akhtabooti-core) and [ratatui](https://github.com/ratatui/ratatui).

Pick a file or folder, scan it, and browse the results in a table sorted by how many PIIs each file contains.

## How it works

1. **Pick**: browse the filesystem and select a file or directory.
2. **Scan**: the selection is handed to `akhtabooti-core` (`search_file` or `search_directory`) on a background thread while a spinner runs.
3. **Review**: results are shown in a table, one row per file, with counts of email addresses, phone numbers, and other PIIs. Rows are sorted by total count and colored by severity:

| Total | Color  |
| ----- | ------ |
| 0     | Green  |
| 1–4   | Yellow |
| 5–9   | Orange |
| 10+   | Red    |

Expanding a row shows the matched emails and phone numbers (up to 8 each), and which other PII types were detected (e.g. "Jordanian Passport").

## Setup

`akhtabooti-core` is pulled from crates.io. Building it requires `cmake` and `libclang` (needed by `ocr-rs`).

### Models

The OCR models come from the [ocr-rs repo (`next` branch)](https://github.com/zibo-chen/ocr-rs/tree/next) and need to be in a `models/` directory in the directory you run the TUI from. From this repo's root, run:

```sh
mkdir -p models && cd models
for f in PP-OCRv6_small_det.mnn PP-OCRv6_small_rec.mnn ppocr_keys_v6_small.txt \
         arabic_PP-OCRv5_mobile_rec_infer.mnn ppocr_keys_arabic.txt; do
  curl -LO "https://raw.githubusercontent.com/zibo-chen/ocr-rs/next/models/$f"
done
```

## Usage

```sh
cargo run --release
```

Model and definition paths are currently relative, so run from the repo root. `definitions.json` and `models/` must both be there.

### Keybindings

**File picker**

| Key     | Action                   |
| ------- | ------------------------ |
| `↑`/`↓` | Move                     |
| `Enter` | Open directory           |
| `Space` | Select file or directory |
| `q`     | Quit                     |

**Results**

| Key     | Action                       |
| ------- | ---------------------------- |
| `↑`/`↓` | Move                         |
| `Enter` | Expand/collapse details      |
| `h`     | Show/hide files with no PIIs |
| `q`     | Quit                         |

## Detection rules

Detection is configured through `definitions.json`, which `akhtabooti-core` loads from the working directory. It must follow core's rule format; see the [akhtabooti-core README](https://crates.io/crates/akhtabooti-core) for details.

## License

Apache-2.0
