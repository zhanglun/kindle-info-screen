//! 每日成语: 内置精选表, 按一年中的第几天轮换, 零联网。
use chrono::Datelike;
use serde_json::Value;
use std::path::Path;

pub fn idiom_of_day(base: &Path) -> Value {
    let table: Value = std::fs::read_to_string(base.join("idioms.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::from_str(include_str!("../resources/idioms.json")).unwrap());
    let arr = table.as_array().cloned().unwrap_or_default();
    if arr.is_empty() {
        return Value::Null;
    }
    let day = chrono::Local::now().ordinal() as usize;
    arr[(day - 1) % arr.len()].clone()
}
