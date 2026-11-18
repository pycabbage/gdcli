use serde_json::{Map, Value};
use std::io::{self, Write};

pub fn compact(value: Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::String(text) => (!text.trim().is_empty()).then_some(Value::String(text)),
        Value::Array(items) => {
            let items: Vec<Value> = items.into_iter().filter_map(compact).collect();
            (!items.is_empty()).then_some(Value::Array(items))
        }
        Value::Object(map) => {
            let map: Map<String, Value> = map
                .into_iter()
                .filter_map(|(key, item)| Some((key, compact(item)?)))
                .collect();
            (!map.is_empty()).then_some(Value::Object(map))
        }
        other => Some(other),
    }
}

fn line(out: &mut impl Write, key: &str, text: &str) -> io::Result<()> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    writeln!(out, "{key}: {}", text.trim().replace('\n', "\n  "))
}

fn quoted(text: &str) -> String {
    let plain =
        !text.is_empty() && !text.contains(|c: char| c.is_whitespace() || c == '"' || c == '=');
    if plain {
        text.to_owned()
    } else {
        Value::from(text).to_string()
    }
}

fn pairs(value: &Value, words: &mut Vec<String>) {
    let Value::Object(map) = value else { return };
    for (name, item) in map {
        match item {
            Value::Object(_) => pairs(item, words),
            Value::String(text) => words.push(format!("{name}={}", quoted(text))),
            Value::Bool(_) | Value::Number(_) => words.push(format!("{name}={item}")),
            Value::Array(_) | Value::Null => {}
        }
    }
}

fn render(out: &mut impl Write, key: &str, value: &Value) -> io::Result<()> {
    match value {
        Value::Array(items) => {
            let singular = key.strip_suffix('s').unwrap_or(key);
            if items.is_empty() {
                return line(out, singular, "none");
            }
            items
                .iter()
                .try_for_each(|item| render(out, singular, item))
        }
        Value::Object(map) => {
            let mut words = Vec::new();
            pairs(value, &mut words);
            if !words.is_empty() {
                line(out, key, &words.join(" "))?;
            }
            map.iter()
                .filter(|(_, item)| item.is_array())
                .try_for_each(|(name, item)| render(out, name, item))
        }
        Value::String(text) => line(out, key, text),
        Value::Bool(_) | Value::Number(_) => line(out, key, &value.to_string()),
        Value::Null => Ok(()),
    }
}

pub fn record(out: &mut impl Write, record: &Value, json: bool) -> io::Result<()> {
    if json {
        return writeln!(out, "{record}");
    }
    let entries = record.as_object().into_iter().flatten();
    entries
        .into_iter()
        .try_for_each(|(key, item)| render(out, key, item))
}

pub fn table(
    out: &mut impl Write,
    header: &str,
    rows: &[Vec<String>],
    json: bool,
) -> io::Result<()> {
    if !json {
        writeln!(out, "{header}")?;
        for row in rows {
            let cells: Vec<String> = row
                .iter()
                .map(|cell| cell.replace(['\t', '\n', '\r'], " "))
                .collect();
            writeln!(out, "{}", cells.join("\t"))?;
        }
        return Ok(());
    }
    let keys: Vec<&str> = header.split('\t').collect();
    let objects: Vec<Value> = rows
        .iter()
        .map(|row| {
            Value::Object(
                keys.iter()
                    .zip(row)
                    .map(|(key, cell)| ((*key).to_owned(), Value::String(cell.clone())))
                    .collect(),
            )
        })
        .collect();
    writeln!(out, "{}", Value::Array(objects))
}
