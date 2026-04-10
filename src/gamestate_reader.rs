use std::fs;

use counter::Counter;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize)]
struct Gamestate {
    epoch: usize,
    initial_month: u32,
    initial_year: i32,
    country_data: std::collections::BTreeMap<u16, u16>,
}

/// All data loaded from gamestate.json.
pub struct GamestateResult {
    pub owners_data: HashMap<u16, u16>,
    pub owns_data: HashMap<u16, u16>,
    pub remaining: HashSet<u16>,
    pub epoch: usize,
    pub initial_month: u32,
    pub initial_year: i32,
}

// Returns all gamestate fields needed by the simulator and reports.
pub fn read_gamestate(requested_round: Option<usize>) -> GamestateResult {
    let round = requested_round.or_else(|| {
        let entries = fs::read_dir("data").ok()?;
        let mut max_round = None;
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if let Ok(r) = name.parse::<usize>() {
                            if fs::metadata(format!("data/{:06}/gamestate.json", r)).is_ok() {
                                max_round = Some(max_round.map_or(r, |m| std::cmp::max(m, r)));
                            }
                        }
                    }
                }
            }
        }
        max_round
    }).expect("No gamestate found. Run with --reset first.");

    let json_str = fs::read_to_string(format!("data/{:06}/gamestate.json", round)).unwrap();
    let data: Gamestate = serde_json::from_str(&json_str).unwrap();

    let owners_data: HashMap<u16, u16> = data.country_data.iter().map(|(&k, &v)| (k, v)).collect();
    let remaining: HashSet<u16> = owners_data.values().cloned().collect();
    let owners_counter = owners_data.values().cloned().collect::<Counter<_>>().into_map();
    let owns_data: HashMap<u16, u16> = owners_data.keys().cloned()
        .map(|k| (k, *owners_counter.get(&k).unwrap_or(&0) as u16))
        .collect();

    GamestateResult {
        owners_data,
        owns_data,
        remaining,
        epoch: data.epoch,
        initial_month: data.initial_month,
        initial_year: data.initial_year,
    }
}
