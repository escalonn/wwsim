use rand::random;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::env;

mod utils;
use utils::{read_closest_data, read_country_data};

mod game_utils;
use game_utils::{compute_neighbors, find_conquered_id, find_conqueror_id};

mod gamestate_reader;
use gamestate_reader::read_gamestate;
///////////////////////////////////////////////////////////////////////////////

pub struct Country {
    name: String,
}

///////////////////////////////////////////////////////////////////////////////

fn main() {
    let args: Vec<String> = env::args().collect();
    let n_runs: usize = args
        .get(1)
        .expect("Provide the number of runs")
        .parse()
        .expect("Not a valid number");

    let country_data = read_country_data();
    let closest_data = read_closest_data();

    // Load the real game's current state as the starting point for all runs.
    let (owners_data_after_log, owns_data_after_log, remaining_after_log, log_epoch) = read_gamestate();

    // Each run is independent; rayon parallelises across available threads.
    (0..n_runs).into_par_iter().for_each(|_| {
        let mut epoch = log_epoch;

        let mut remaining = remaining_after_log.clone();
        let mut owners_data = owners_data_after_log.clone();
        let mut owns_data = owns_data_after_log.clone();

        let owners_ref = &mut owners_data;
        let owns_ref = &mut owns_data;
        let remaining_ref = &mut remaining;

        while remaining_ref.len() > 1 {
            epoch += 1;

            // Independence chance shrinks as the game goes on.
            let independence_chance = 1.0 / (12.0 + (epoch as f64 / 10.0));
            let neighbors = compute_neighbors(owners_ref, &closest_data);
            let conqueror_id = find_conqueror_id(owners_ref, &neighbors);

            if random::<f64>() < independence_chance {
                independence(conqueror_id, owners_ref, owns_ref, remaining_ref);
            } else {
                let conquered_id = find_conquered_id(conqueror_id, owners_ref, &neighbors);
                conquer(
                    conqueror_id,
                    conquered_id,
                    owners_ref,
                    owns_ref,
                    remaining_ref,
                );
            }
        }

        println!("{}", country_data[remaining.iter().next().unwrap()].name);
    })
}

///////////////////////////////////////////////////////////////////////////////

// A territory breaks away from its owner and becomes sovereign again.
fn independence(
    indep_terr_id: u16,
    owners_data: &mut HashMap<u16, u16>,
    owns_data: &mut HashMap<u16, u16>,
    remaining: &mut HashSet<u16>,
) {
    let old_owner_id = owners_data[&indep_terr_id];

    owners_data.insert(indep_terr_id, indep_terr_id);
    *owns_data.entry(indep_terr_id).or_insert(0) += 1;
    *owns_data.entry(old_owner_id).or_insert(0) -= 1;

    // The newly independent territory re-enters the game as its own country.
    if owns_data[&indep_terr_id] == 1 {
        remaining.insert(indep_terr_id);
    }

    if owns_data[&old_owner_id] == 0 {
        remaining.remove(&old_owner_id);
    }
}

// The country owning `conqueror_terr_id` absorbs the country owning `conquered_terr_id`.
fn conquer(
    conqueror_terr_id: u16,
    conquered_terr_id: u16,
    owners_data: &mut HashMap<u16, u16>,
    owns_data: &mut HashMap<u16, u16>,
    remaining: &mut HashSet<u16>,
) {
    let original_conqueror_id = owners_data[&conqueror_terr_id];
    let original_conquered_id = owners_data[&conquered_terr_id];

    *owns_data.entry(original_conqueror_id).or_insert(0) += 1;
    *owns_data.entry(original_conquered_id).or_insert(0) -= 1;

    owners_data.insert(conquered_terr_id, owners_data[&conqueror_terr_id]);

    if owns_data[&original_conquered_id] == 0 {
        remaining.remove(&original_conquered_id);
    }
}
