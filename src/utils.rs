use std::collections::HashMap;
use std::fs;

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

// Format: `id; name; longitude; latitude` — coordinates are unused by the simulator.
pub fn read_country_data() -> HashMap<u16, Country> {
    let lines = read_lines("data/country_data.csv");
    lines
        .iter()
        .filter_map(|line| {
            let spl: Vec<&str> = line.split(";").collect();
            let id_str = spl.first()?;
            if id_str.is_empty() { return None; }
            let id: u16 = id_str.parse().ok()?;
            let name: String = spl.get(1)?.to_string();
            Some((id, Country { name }))
        })
        .collect()
}
