use rand::random;
use rayon::prelude::*;
use std::env;

mod utils;
use utils::{read_country_data, read_targets_data};

mod game_utils;
use game_utils::{find_attack_targets, perform_conquest, perform_riot};

mod gamestate_reader;
use gamestate_reader::read_gamestate;

mod scraper;
use scraper::update_gamestate;
///////////////////////////////////////////////////////////////////////////////

pub struct Country {
    name: String,
}

///////////////////////////////////////////////////////////////////////////////

fn main() {
    let args: Vec<String> = env::args().collect();
    let is_reset = args.iter().any(|arg| arg == "--reset");
    let is_local = args.iter().any(|arg| arg == "--local");
    if is_reset {
        scraper::reset_gamestate().expect("Failed to reset gamestate");
        // Don't run simulations on a reset unless runs are also requested
        if !args.iter().any(|a| a.parse::<usize>().is_ok()) {
            return;
        }
    }

    let runs_unparsed = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with("--"))
        .expect("Provide the number of runs");

    let n_runs: usize = runs_unparsed
        .parse()
        .expect("Not a valid number");

    if !is_local {
        if let Err(e) = update_gamestate() {
            eprintln!("Scraper encountered a critical error: {}", e);
            std::process::exit(1);
        }
    }

    let country_data = read_country_data();
    let targets_data = read_targets_data();

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

            let active_territories_ids: Vec<u16> = owners_ref.keys().copied().collect();
            let mut rng = rand::thread_rng();
            use rand::seq::SliceRandom;
            let chosen_id: u16 = *active_territories_ids.choose(&mut rng).unwrap();

            let is_eliminated = *owns_ref.get(&chosen_id).unwrap_or(&0) == 0;
            if is_eliminated {
                let independence_chance = 1.0 / (12.0 + (epoch as f64 / 10.0));
                if random::<f64>() < independence_chance {
                    perform_riot(chosen_id, owners_ref, owns_ref, &targets_data, remaining_ref);
                    continue;
                }
            }

            let targets = find_attack_targets(chosen_id, owners_ref, &targets_data);
            if targets.is_empty() {
                continue;
            }
            let target_id = *targets.choose(&mut rng).unwrap();
            perform_conquest(
                chosen_id,
                target_id,
                owners_ref,
                owns_ref,
                &targets_data,
                remaining_ref,
            );
        }

        println!("{}", country_data[remaining.iter().next().unwrap()].name);
    })
}
