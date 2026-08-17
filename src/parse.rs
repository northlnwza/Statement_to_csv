// Turn extracted text lines into structured transactions.

#[derive(Debug)]
pub struct Txn {
    pub date: String,        // ISO yyyy-mm-dd
    pub time: String,        // HH:MM
    pub channel: String,
    pub ttype: String,       // Thai transaction type word, if detected
    pub description: String,
    pub withdrawal: Option<f64>,
    pub deposit: Option<f64>,
    pub balance: f64,
}

// KBank transaction type keywords, longest-first so e.g. รับโอนเงิน wins over โอนเงิน.
const TYPE_WORDS: &[&str] = &[
    "รับโอนเงิน", "โอนเงิน", "ชำระเงิน", "ถอนเงิน", "ฝากเงิน", "ดอกเบี้ย", "ค่าธรรมเนียม",
];

/// Detect the bank from the statement header and dispatch to its parser.
/// `bank` is `None` when the layout is not recognized (so an empty result can be
/// told apart from a genuinely empty statement).
pub fn parse(lines: &[String]) -> (Option<&'static str>, Vec<Txn>) {
    let has = |needle: &str| lines.iter().any(|l| l.contains(needle));

    if has("SIAM COMMERCIAL") || has("ไทยพาณิชย์") {
        return (Some("SCB"), parse_scb(lines));
    }
    if has("KBPDF") || has("ธนาคารกสิกรไทย") || has("KASIKORNBANK")
        || has("K PLUS") || has("ยอดยกมา")
    {
        return (Some("KBank"), parse_kbank(lines));
    }
    (None, Vec::new())
}

fn parse_kbank(lines: &[String]) -> Vec<Txn> {
    let mut prev_balance: Option<f64> = None;
    let mut txns = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];

        // Opening "balance brought forward" row: `DD-MM-YY\t<num>ยอดยกมา`
        if let Some(bal) = opening_balance(line) {
            prev_balance = Some(bal);
            i += 1;
            continue;
        }

        match txn_header(line) {
            None => { i += 1; }
            Some((date, time, rest)) => {
                // Gather this row plus any wrapped continuation lines.
                let mut blob_parts: Vec<String> = Vec::new();
                let fields: Vec<&str> = rest.split('\t').filter(|s| !s.is_empty()).collect();
                let (channel, balance) = match fields.first() {
                    Some(first) => match split_trailing_amount(first) {
                        Some((chan, bal)) => (chan.trim().to_string(), bal),
                        None => (first.trim().to_string(), f64::NAN),
                    },
                    None => (String::new(), f64::NAN),
                };
                for f in fields.iter().skip(1) {
                    blob_parts.push((*f).to_string());
                }

                // Continuation lines: until the next header / opening row / end.
                let mut j = i + 1;
                while j < lines.len()
                    && txn_header(&lines[j]).is_none()
                    && opening_balance(&lines[j]).is_none()
                    && !is_footer(&lines[j])
                {
                    blob_parts.push(lines[j].replace('\t', " "));
                    j += 1;
                }
                i = j;

                if balance.is_nan() {
                    continue; // couldn't locate the running balance; skip malformed row
                }

                let mut description = clean(&blob_parts.join(" "));
                // Strip a trailing printed amount token (we derive the real amount below).
                if let Some((head, _)) = split_trailing_amount(&description) {
                    description = head.trim().to_string();
                }
                let ttype = detect_type(&description);

                let (withdrawal, deposit) = match prev_balance {
                    Some(prev) => {
                        let delta = round2(balance - prev);
                        if delta < 0.0 {
                            (Some(-delta), None)
                        } else if delta > 0.0 {
                            (None, Some(delta))
                        } else {
                            (None, None)
                        }
                    }
                    None => (None, None),
                };
                prev_balance = Some(balance);

                txns.push(Txn {
                    date,
                    time,
                    channel,
                    ttype,
                    description,
                    withdrawal,
                    deposit,
                    balance,
                });
            }
        }
    }

    txns
}

// ---------------------------------------------------------------------------
// SCB (Siam Commercial Bank) savings-account statement
//
// Row layout (one transaction per line, occasionally wrapped):
//   DD/MM/YY HH:MM Xn \t CHANNEL \t <amount><balance><description>
// where the code Xn is X1 = credit (money in) / X2 = debit (money out), and the
// amount + running balance are two `1,234.56` numbers (sometimes glued together)
// followed by the description. Amount/direction are confirmed by balance delta.
// ---------------------------------------------------------------------------

// SCB description keywords, longest-first.
const SCB_TYPE_WORDS: &[&str] = &[
    "รับโอนจาก", "โอนไป", "จ่ายบิล", "เติมเงิน", "ถอนเงิน", "ฝากเงิน",
    "ดอกเบี้ย", "ค่าธรรมเนียม", "PromptPay", "DCP",
];

fn parse_scb(lines: &[String]) -> Vec<Txn> {
    let mut prev_balance: Option<f64> = None;
    let mut txns = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];

        if let Some(bal) = scb_opening_balance(line) {
            prev_balance = Some(bal);
            i += 1;
            continue;
        }

        let (date, time, code, rest) = match scb_header(line) {
            Some(h) => h,
            None => { i += 1; continue; }
        };

        // channel = first tab field; the rest is the number+description blob.
        let fields: Vec<&str> = rest.split('\t').filter(|s| !s.is_empty()).collect();
        let (channel, mut blob) = match fields.split_first() {
            Some((first, tail)) => {
                // If the first field already contains digits it's the blob (no channel).
                if first.chars().any(|c| c.is_ascii_digit()) {
                    (String::new(), fields.join(" "))
                } else {
                    (first.trim().to_string(), tail.join(" "))
                }
            }
            None => (String::new(), String::new()),
        };

        // Wrapped continuation lines.
        let mut j = i + 1;
        while j < lines.len()
            && scb_header(&lines[j]).is_none()
            && scb_opening_balance(&lines[j]).is_none()
            && !scb_is_footer(&lines[j])
        {
            blob.push(' ');
            blob.push_str(&lines[j].replace('\t', " "));
            j += 1;
        }
        i = j;

        // Pull the two leading numbers: amount then running balance.
        let blob_trim = blob.trim_start();
        let (amount, after_amt) = match leading_number(blob_trim) {
            Some(x) => x,
            None => continue,
        };
        let rest2 = blob_trim[after_amt..].trim_start();
        let (balance, after_bal) = match leading_number(rest2) {
            Some(x) => x,
            None => continue,
        };
        let description = clean(&rest2[after_bal..]);
        let ttype = detect_type_in(&description, SCB_TYPE_WORDS);

        // Direction from the X1/X2 code, cross-checked against the balance delta.
        let credit = code == "X1";
        let (withdrawal, deposit) = match prev_balance {
            Some(prev) => {
                let delta = round2(balance - prev);
                if delta > 0.0 { (None, Some(delta)) }
                else if delta < 0.0 { (Some(-delta), None) }
                else if credit { (None, Some(amount)) } else { (Some(amount), None) }
            }
            None => if credit { (None, Some(amount)) } else { (Some(amount), None) },
        };
        prev_balance = Some(balance);

        txns.push(Txn {
            date,
            time,
            channel,
            ttype,
            description,
            withdrawal,
            deposit,
            balance,
        });
    }

    txns
}

/// Match `DD/MM/YYHH:MMXn` at the start; return (iso_date, time, code, rest).
fn scb_header(line: &str) -> Option<(String, String, String, &str)> {
    let b = line.as_bytes();
    if b.len() < 15 { return None; }
    let d = |k: usize| b[k].is_ascii_digit();
    if !(d(0) && d(1) && b[2] == b'/' && d(3) && d(4) && b[5] == b'/' && d(6) && d(7)
        && d(8) && d(9) && b[10] == b':' && d(11) && d(12) && b[13] == b'X' && d(14))
    {
        return None;
    }
    let iso = iso_date(&line[0..8]);
    let time = line[8..13].to_string();
    let code = line[13..15].to_string();
    let rest = line[15..].trim_start_matches('\t');
    Some((iso, time, code, rest))
}

/// SCB opening row: `...BALANCE BROUGHT FORWARD)<number>` (trailing balance).
fn scb_opening_balance(line: &str) -> Option<f64> {
    if !(line.contains("BROUGHT FORWARD") || line.contains("ยอดเงินคงเหลือยกมา")) {
        return None;
    }
    split_trailing_amount(line).map(|(_, v)| v)
}

fn scb_is_footer(line: &str) -> bool {
    line.starts_with("TOTAL")
        || line.contains("auto-generated")
        || line.contains("ออกโดยระบบ")
        || line.starts_with("หน้า")
}

/// Match `Xn` transaction type / parse a `1,234.56` number from the START of a
/// string; return (value, byte_index_after_number). Requires exactly `.NN`.
fn leading_number(s: &str) -> Option<(f64, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b',') { i += 1; }
    if i == 0 || i + 3 > b.len() || b[i] != b'.' { return None; }
    if !(b[i + 1].is_ascii_digit() && b[i + 2].is_ascii_digit()) { return None; }
    let end = i + 3;
    let val = parse_amount(&s[..end])?;
    Some((val, end))
}

/// Match `DD-MM-YYHH:MM` at the start; return (iso_date, time, rest_after_time).
fn txn_header(line: &str) -> Option<(String, String, &str)> {
    let b = line.as_bytes();
    if b.len() < 13 { return None; }
    let d = |k: usize| b[k].is_ascii_digit();
    if !(d(0) && d(1) && b[2] == b'-' && d(3) && d(4) && b[5] == b'-'
        && d(6) && d(7) && d(8) && d(9) && b[10] == b':' && d(11) && d(12))
    {
        return None;
    }
    let iso = iso_date(&line[0..8]);
    let time = line[8..13].to_string();
    let rest = line[13..].trim_start_matches('\t');
    Some((iso, time, rest))
}

/// Opening row `DD-MM-YY\t<number>ยอดยกมา` → the brought-forward balance.
fn opening_balance(line: &str) -> Option<f64> {
    if !line.contains("ยอดยกมา") { return None; }
    let b = line.as_bytes();
    if b.len() < 9 { return None; }
    let d = |k: usize| b[k].is_ascii_digit();
    if !(d(0) && d(1) && b[2] == b'-' && d(3) && d(4) && b[5] == b'-' && d(6) && d(7)) {
        return None;
    }
    let rest = line[8..].trim_start_matches('\t');
    leading_amount(rest)
}

fn is_footer(line: &str) -> bool {
    line.starts_with("KBPDF") || line.contains("Contact Center") || line.contains("ออกโดย")
}

fn detect_type(desc: &str) -> String {
    detect_type_in(desc, TYPE_WORDS)
}

fn detect_type_in(desc: &str, words: &[&str]) -> String {
    for w in words {
        if desc.contains(w) {
            return (*w).to_string();
        }
    }
    String::new()
}

/// "05-01-26" or "05/05/26" -> "2026-01-05" (two-digit year assumed 2000s).
fn iso_date(ddmmyy: &str) -> String {
    let p: Vec<&str> = ddmmyy.split(['-', '/']).collect();
    if p.len() == 3 {
        format!("20{}-{}-{}", p[2], p[1], p[0])
    } else {
        ddmmyy.to_string()
    }
}

/// Split a string ending in `<...><number>` into (prefix, value), where the
/// trailing number looks like `1,234.56`. Returns None if no such suffix.
fn split_trailing_amount(s: &str) -> Option<(&str, f64)> {
    let bytes = s.as_bytes();
    let mut start = bytes.len();
    while start > 0 {
        let c = bytes[start - 1];
        if c.is_ascii_digit() || c == b',' || c == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    let suffix = &s[start..];
    let val = parse_amount(suffix)?;
    Some((&s[..start], val))
}

/// Parse a number from the START of a string (e.g. "1,068.08ยอดยกมา").
fn leading_amount(s: &str) -> Option<f64> {
    let end = s
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == ',' || *c == '.')
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    parse_amount(&s[..end])
}

/// Validate and parse "1,234.56" → 1234.56. Requires a `.NN` cents part.
fn parse_amount(s: &str) -> Option<f64> {
    if !s.contains('.') { return None; }
    let cleaned: String = s.chars().filter(|c| *c != ',').collect();
    let (int_part, frac) = cleaned.split_once('.')?;
    if int_part.is_empty() || frac.len() != 2 { return None; }
    if !int_part.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        let c = if c == '\t' || c == '\n' { ' ' } else { c };
        if c == ' ' {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
