use std::io::{self, Write};

fn plain(value: &str) -> String {
    value.replace(['\t', '\n', '\r'], " ")
}

pub fn row(out: &mut impl Write, columns: &[&str]) -> io::Result<()> {
    let line: Vec<String> = columns.iter().map(|column| plain(column)).collect();
    writeln!(out, "{}", line.join("\t"))
}

pub fn field(out: &mut impl Write, key: &str, value: &str) -> io::Result<()> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    writeln!(out, "{key}: {}", value.replace('\n', "\n  "))
}

pub fn field_opt(out: &mut impl Write, key: &str, value: Option<&str>) -> io::Result<()> {
    value.map_or(Ok(()), |value| field(out, key, value))
}
