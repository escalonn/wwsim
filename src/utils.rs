use std::collections::HashMap;
use std::fs;
use plotters::prelude::RGBColor;

use crate::Country;

fn read_lines(path: &str) -> Vec<String> {
    let file_str = fs::read_to_string(path).unwrap();
    let lines = file_str
        .split('\n')
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty());
    lines.collect()
}

pub fn read_targets_data() -> HashMap<u16, Vec<u16>> {
    let file_str = fs::read_to_string("data/targets.json").expect("Failed to read data/targets.json");
    let arrays: Vec<Vec<u16>> = serde_json::from_str(&file_str).expect("Failed to parse data/targets.json");
    arrays.into_iter().enumerate().map(|(i, arr)| (i as u16, arr)).collect()
}

fn hex_to_rgb(hex: &str) -> Option<RGBColor> {
    let hex = hex.trim();
    if hex.starts_with('#') && hex.len() == 7 {
        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
        Some(RGBColor(r, g, b))
    } else {
        None
    }
}

// Format: `id; name; longitude; latitude` — coordinates are unused by the simulator.
pub fn read_country_data() -> HashMap<u16, Country> {
    let lines = read_lines("data/country_data.csv");

    let color_map: HashMap<String, String> =
        if let Ok(content) = fs::read_to_string("data/countries.json") {
            let val: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::Value::Null);
            if let Some(arr) = val.as_array() {
                arr.iter()
                    .filter_map(|item| {
                        let pair = item.as_array()?;
                        let name = pair.get(0)?.as_str()?;
                        let color = pair.get(1)?.as_object()?.get("color")?.as_str()?;
                        Some((name.to_string(), color.to_string()))
                    })
                    .collect()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

    lines
        .iter()
        .filter_map(|line| {
            let spl: Vec<&str> = line.split(";").collect();
            let id_str = spl.first()?;
            if id_str.is_empty() {
                return None;
            }
            let id: u16 = id_str.parse().ok()?;
            let name: String = spl.get(1)?.trim().to_string();
            let color = color_map.get(&name).and_then(|h| hex_to_rgb(h));

            Some((id, Country { name, color }))
        })
        .collect()
}
