// Render transactions as RFC-4180 CSV with a UTF-8 BOM (so Excel/Sheets read Thai).
use crate::parse::Txn;

const HEADER: &[&str] = &[
    "Date", "Time", "Channel", "Type", "Description", "Withdrawal", "Deposit", "Balance",
];

pub fn render(txns: &[Txn]) -> String {
    let mut out = String::from("\u{feff}"); // BOM
    out.push_str(&row(HEADER.iter().map(|s| s.to_string())));
    for t in txns {
        out.push_str(&row([
            t.date.clone(),
            t.time.clone(),
            t.channel.clone(),
            t.ttype.clone(),
            t.description.clone(),
            money(t.withdrawal),
            money(t.deposit),
            format!("{:.2}", t.balance),
        ]));
    }
    out
}

fn money(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:.2}"),
        None => String::new(),
    }
}

fn row(fields: impl IntoIterator<Item = String>) -> String {
    let mut line = String::new();
    for (i, f) in fields.into_iter().enumerate() {
        if i > 0 {
            line.push(',');
        }
        line.push_str(&escape(&f));
    }
    line.push_str("\r\n");
    line
}

fn escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}
