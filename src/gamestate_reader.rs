use std::fs;

use counter::Counter;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize)]
struct Gamestate {
    epoch: usize,
    // Maps territory ID → owner ID (both stored as strings in JSON).
    country_data: HashMap<String, String>,
}

// Returns: (owners_data, owns_data, remaining, epoch)
//   owners_data: territory → current owner
//   owns_data:   country   → number of territories held
//   remaining:   set of countries still in the game
pub fn read_gamestate() -> (HashMap<u16, u16>, HashMap<u16, u16>, HashSet<u16>, usize) {
    let json_str = fs::read_to_string("data/gamestate.json").unwrap();
    let data: Gamestate = serde_json::from_str(&json_str).unwrap();
    let owners_data: HashMap<u16, u16> = data.country_data.iter()
        .map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap()))
        .collect();
    // A country is "remaining" if it owns at least one territory.
    let remaining: HashSet<u16> = owners_data.values().cloned().collect();
    let owners_counter = owners_data.values().cloned().collect::<Counter<_>>().into_map();
    let owns_data: HashMap<u16, u16> = owners_data.keys().cloned()
        .map(|k| (k, *owners_counter.get(&k).unwrap_or(&0) as u16))
        .collect();

    (owners_data, owns_data, remaining, data.epoch)
}
