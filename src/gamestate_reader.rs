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
pub fn read_gamestate() -> GamestateResult {
    let json_str = fs::read_to_string("data/gamestate.json").unwrap();
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
