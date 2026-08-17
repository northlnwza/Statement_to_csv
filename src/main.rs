// bankcsv — read password-protected KBank (Kasikorn) PDF statements and write
// one CSV per statement (date, time, channel, type, description, withdrawal,
// deposit, balance).
//
// Usage: bankcsv <pdf-or-dir>... [-p PASSWORD] [-o OUTDIR]
//   - Multiple PDFs and/or directories may be given; directories are scanned
//     for *.pdf. Each PDF produces its own CSV named after it (statement.pdf ->
//     statement.csv), written next to the PDF or into OUTDIR if given.
//   - Password: -p/--password, else env BANK_PDF_PASSWORD, else empty. The same
//     password is applied to every file (KBank uses one password per holder).
//
// How it works:
//   1. pdf-rs opens + decrypts the AES-encrypted PDF.
//   2. We walk each page's content operators, decode glyphs via each font's
//      ToUnicode map, and rebuild positioned text lines (tabs mark column gaps).
//   3. Lines starting with `DD-MM-YYHH:MM` begin a transaction; following lines
//      without that prefix are wrapped-description continuations.
//   4. The signed amount is derived from the running balance delta (robust even
//      when the printed amount is glued to text); the sign decides withdrawal vs
//      deposit.

mod extract;
mod parse;
mod csv;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut inputs: Vec<String> = Vec::new();
    let mut password: Option<String> = None;
    let mut outdir: Option<String> = None;
    let mut dump = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => { i += 1; outdir = args.get(i).cloned(); }
            "-p" | "--password" => { i += 1; password = args.get(i).cloned(); }
            "--dump" => dump = true,
            "-h" | "--help" => { eprintln!("{USAGE}"); return ExitCode::SUCCESS; }
            s if s.starts_with('-') => { eprintln!("unknown flag: {s}\n\n{USAGE}"); return ExitCode::FAILURE; }
            s => inputs.push(s.to_string()),
        }
        i += 1;
    }

    if inputs.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    let password = password
        .or_else(|| std::env::var("BANK_PDF_PASSWORD").ok())
        .unwrap_or_default();

    let pdfs = match collect_pdfs(&inputs) {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => { eprintln!("no PDF files found in: {}", inputs.join(", ")); return ExitCode::FAILURE; }
        Err(e) => { eprintln!("{e}"); return ExitCode::FAILURE; }
    };

    if let Some(dir) = &outdir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("create out dir {dir}: {e}");
            return ExitCode::FAILURE;
        }
    }

    let mut ok = 0usize;
    let mut failed = 0usize;
    for pdf in &pdfs {
        let name = pdf.file_name().and_then(|s| s.to_str()).unwrap_or("?");
        let lines = match extract::extract_lines(&pdf.to_string_lossy(), &password) {
            Ok(l) => l,
            Err(e) => { eprintln!("{name}: FAILED to read ({e})"); failed += 1; continue; }
        };
        if dump {
            println!("########## {name} : {} lines ##########", lines.len());
            for l in &lines { println!("{l}"); }
            ok += 1;
            continue;
        }
        let (bank, txns) = parse::parse(&lines);
        let bank = match bank {
            Some(b) => b,
            None => {
                eprintln!("{name}: FAILED — unrecognized statement layout");
                failed += 1;
                continue;
            }
        };
        // A recognized statement with no rows is a valid empty month, not a failure.

        let out_path = csv_path(pdf, outdir.as_deref());
        match std::fs::write(&out_path, csv::render(&txns).as_bytes()) {
            Ok(()) => {
                let note = if txns.is_empty() { "  (0 transactions — empty statement)".to_string() }
                           else { format!("  ({} transactions)", txns.len()) };
                println!("{name}  [{bank}]  ->  {}{note}", out_path.display());
                ok += 1;
            }
            Err(e) => { eprintln!("{name}: write {} failed ({e})", out_path.display()); failed += 1; }
        }
    }

    eprintln!("done: {ok} ok, {failed} failed");
    if failed > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// Output path for a PDF: same stem with `.csv`, in OUTDIR if given else beside it.
fn csv_path(pdf: &Path, outdir: Option<&str>) -> PathBuf {
    let stem = pdf.file_stem().and_then(|s| s.to_str()).unwrap_or("statement");
    let file = format!("{stem}.csv");
    match outdir {
        Some(dir) => Path::new(dir).join(file),
        None => pdf.with_file_name(file),
    }
}

/// Expand inputs (files or directories) into a sorted, de-duplicated PDF list.
fn collect_pdfs(inputs: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut out: Vec<PathBuf> = Vec::new();
    for inp in inputs {
        let p = Path::new(inp);
        if p.is_dir() {
            let rd = std::fs::read_dir(p).map_err(|e| format!("read dir {inp}: {e}"))?;
            for entry in rd.flatten() {
                let path = entry.path();
                if is_pdf(&path) {
                    out.push(path);
                }
            }
        } else if p.is_file() {
            out.push(p.to_path_buf());
        } else {
            return Err(format!("not found: {inp}"));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn is_pdf(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

const USAGE: &str = "\
bankcsv <pdf-or-dir>... [-p PASSWORD] [-o OUTDIR]

  Reads password-protected KBank PDF statements and writes one CSV per file
  (statement.pdf -> statement.csv). Prints which CSV came from which PDF.

  -p, --password   PDF password (or set BANK_PDF_PASSWORD; same for all files)
  -o, --out        directory to write CSVs into (default: beside each PDF)

Examples:
  bankcsv statement.pdf -p <PDF_PASSWORD>
  bankcsv jan.pdf apr.pdf -p <PDF_PASSWORD>
  BANK_PDF_PASSWORD=<PDF_PASSWORD> bankcsv ./statements -o ./csv";
