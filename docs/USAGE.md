# bankcsv — Run Manual

Turn password-protected Thai bank PDF statements (KBank, SCB) into CSV.
Output columns: `Source, Date, Time, Channel, Type, Description, Withdrawal, Deposit, Balance`

---

## 1. Location

```
Statement_to_csv/
├── data/statements/         <- private PDFs (ignored by Git)
├── data/output/             <- generated CSVs (ignored by Git)
├── docs/USAGE.md
├── src/                     <- Rust program
└── target/release/bankcsv   <- built binary
```

Run commands from `Statement_to_csv`.

```sh
cd /Users/xynorith/Desktop/STUFF/Statement_to_csv
```

---

## 2. Build (only first time, or after code change)

```sh
cargo build --release
```

Needs Rust. No Rust? Install:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Binary already built at `target/release/bankcsv`. Skip build if unchanged.

---

## 3. Run

Syntax:

```
bankcsv <pdf-or-folder>... [-p PASSWORD] [-o OUTDIR]
```

- `-p, --password` — PDF password. Same password used for all files.
- `-o, --out` — folder to drop CSVs in. One CSV per PDF (`jan.pdf` → `jan.csv`). Default: beside each PDF.

Put PDFs in `data/statements/`; generated files belong in `data/output/`.

### One file, print to screen

```sh
./target/release/bankcsv data/statements/statement.pdf -p '<PDF_PASSWORD>'
```

### One file, write CSV next to it

```sh
./target/release/bankcsv data/statements/statement.pdf -p '<PDF_PASSWORD>' -o data/output/statement.csv
```

Makes `data/output/statement.csv`.

### Many files at once

```sh
./target/release/bankcsv data/statements/jan.pdf data/statements/feb.pdf -p '<PDF_PASSWORD>' -o data/output/year.csv
```

### Whole folder (every *.pdf inside)

```sh
./target/release/bankcsv data/statements -p '<PDF_PASSWORD>' -o data/output
```

Reads all PDFs in `data/statements`, writes all CSVs to `data/output`.

---

## 4. Password without shell history

Put password in env var instead of `-p`:

```sh
BANK_PDF_PASSWORD='<PDF_PASSWORD>' ./target/release/bankcsv data/statements -o data/output
```

---

## 5. Open in Google Sheets

File → Import → Upload the CSV → Separator type: **Comma**.
CSV is UTF-8 + BOM, so Thai renders right and amounts stay numeric.

---

## 6. Debug a new/unsupported bank

```sh
./target/release/bankcsv newbank.pdf -p PASSWORD --dump
```

Prints extracted text lines. Use to write a new parser in `src/parse.rs`.

---

## 7. Trouble

| Problem | Fix |
|---|---|
| `command not found` | Run from `bankcsv/`, use `./target/release/bankcsv` |
| no binary | `cargo build --release` |
| wrong/no output | check password; check PDF is KBank or SCB |
| garbled Thai in Excel | import as UTF-8, comma separator |
