# bankcsv

Read password-protected Thai **bank PDF statements** and emit a CSV of
transactions you can paste straight into Google Sheets.

**Supported banks (auto-detected per file):**
- **KBank** (Kasikorn) savings statement
- **SCB** (Siam Commercial Bank) savings statement

The bank is detected from each file's header, so you can mix them in one run;
each file's CSV is labelled with its detected bank in the console output.

Columns: `Source, Date, Time, Channel, Type, Description, Withdrawal, Deposit, Balance`

Multiple statements can be merged into one CSV (sorted by date, with a `Source`
column naming each row's file).

- Decrypts the AES-encrypted PDF (`pdf` crate) with the file password.
- Rebuilds positioned text from the PDF content stream (Thai included).
- Derives each amount from the **running-balance delta**, so it's correct even
  when the printed number is glued to surrounding text. The delta's sign decides
  withdrawal vs deposit.

## Build

```sh
cargo build --release
```

## Run

```sh
# one file to stdout
./target/release/bankcsv data/statements/statement.pdf -p '<PDF_PASSWORD>'

# one file to a CSV
./target/release/bankcsv data/statements/statement.pdf -p '<PDF_PASSWORD>' -o data/output/spending.csv

# merge several statements into one CSV (sorted by date)
./target/release/bankcsv data/statements/jan.pdf data/statements/feb.pdf -p '<PDF_PASSWORD>' -o data/output/year.csv

# point at a folder: every *.pdf in it is read
./target/release/bankcsv data/statements -p '<PDF_PASSWORD>' -o data/output/all.csv

# password via env instead of argv (keeps it out of shell history)
BANK_PDF_PASSWORD='<PDF_PASSWORD>' ./target/release/bankcsv data/statements -o data/output/all.csv
```

The same password is applied to every file. Files can be given in any order —
each statement carries its own opening balance, so amounts stay correct.

The CSV is UTF-8 with a BOM so Excel/Sheets render Thai correctly. Amounts have
no thousands separators so spreadsheets treat them as numbers.

## Import to Google Sheets

File → Import → Upload `spending.csv` → *Separator type: Comma*. Or just open the
file and copy/paste.

## Verification

The tool's totals reconcile with the statement footer
(`รวมถอนเงิน` / `รวมฝากเงิน`): for the sample file, 24 withdrawals = 3131.00 and
4 deposits = 3133.28, matching the bank's printed totals.

## Adding another bank

`src/parse.rs` has a small per-bank parser plus a `parse()` dispatcher that
picks one from the header text. To add a bank, dump its layout and write a
parser the same way:

```sh
./target/release/bankcsv newbank.pdf -p PASSWORD --dump   # prints extracted lines
```

## Notes / limits

- Tuned for KBank and SCB savings-account layouts (iText-produced PDFs). Other
  banks/templates need a parser added (see above).
- Amounts and direction are derived from the running-balance delta, so they're
  validated against the statement's own running balance.
- The first transaction needs the opening balance row to be present
  (`ยอดยกมา` for KBank, `BALANCE BROUGHT FORWARD` for SCB) — it always is.
