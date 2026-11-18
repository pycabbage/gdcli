use serde_json::Value;
use std::io::{self, Write};

pub fn row(out: &mut impl Write, columns: &[&str]) -> io::Result<()> {
    let line: Vec<String> = columns
        .iter()
        .map(|column| column.replace(['\t', '\n', '\r'], " "))
        .collect();
    writeln!(out, "{}", line.join("\t"))
}

pub fn field(out: &mut impl Write, key: &str, value: &str) -> io::Result<()> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    writeln!(out, "{key}: {}", value.replace('\n', "\n  "))
}

pub fn flatten(out: &mut impl Write, key: &str, value: &Value) -> io::Result<()> {
    let nested = |name: &str| {
        if key.is_empty() {
            name.to_owned()
        } else {
            format!("{key}.{name}")
        }
    };
    match value {
        Value::Object(map) => map
            .iter()
            .try_for_each(|(name, item)| flatten(out, &nested(name), item)),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(index, item)| flatten(out, &nested(&index.to_string()), item)),
        Value::String(text) => field(out, key, text),
        Value::Null => Ok(()),
        other => field(out, key, &other.to_string()),
    }
}
