use rand::random;
use rayon::prelude::*;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod utils;
use utils::{read_country_data, read_targets_data};

mod game_utils;
use game_utils::{find_attack_targets, perform_conquest, perform_riot};

mod gamestate_reader;
use gamestate_reader::{read_gamestate, GamestateResult};

mod scraper;
use scraper::update_gamestate;
///////////////////////////////////////////////////////////////////////////////

pub struct Country {
    name: String,
}

///////////////////////////////////////////////////////////////////////////////

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    let mut parts = Vec::new();
    if hours > 0 { parts.push(format!("{} hour{}", hours, if hours == 1 { "" } else { "s" })); }
    if mins  > 0 { parts.push(format!("{} minute{}", mins, if mins == 1 { "" } else { "s" })); }
    if s > 0 || parts.is_empty() { parts.push(format!("{} second{}", s, if s == 1 { "" } else { "s" })); }
    parts.join(", ")
}

/// Convert an epoch count to a calendar month/year string.
/// Each epoch is one turn ≈ one month for display purposes.
fn epoch_to_date(epoch: usize, initial_month: u32, initial_year: i32) -> String {
    let months_elapsed = epoch as i64;
    let total_months = (initial_year as i64) * 12 + (initial_month as i64 - 1) + months_elapsed;
    let year = total_months / 12;
    let month = (total_months % 12) as u32 + 1;
    let month_name = match month {
        1  => "January",   2  => "February", 3  => "March",
        4  => "April",     5  => "May",       6  => "June",
        7  => "July",      8  => "August",    9  => "September",
        10 => "October",   11 => "November",  12 => "December",
        _  => "Unknown",
    };
    format!("{} {}", month_name, year)
}

/// Format a number of average turns (months) into days/hours.
fn months_to_duration_str(avg_turns: f64) -> String {
    let total_hours = avg_turns; // 1 turn = 1 hour as stated in the spec
    let days = (total_hours / 24.0) as u64;
    let hours = (total_hours % 24.0) as u64;
    let mut parts = Vec::new();
    if days > 0 { parts.push(format!("{} day{}", days, if days == 1 { "" } else { "s" })); }
    if hours > 0 { parts.push(format!("{} hour{}", hours, if hours == 1 { "" } else { "s" })); }
    if parts.is_empty() { parts.push("less than 1 hour".to_string()); }
    parts.join(", ")
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let is_reset = args.iter().any(|arg| arg == "--reset");
    let is_local = args.iter().any(|arg| arg == "--local");
    let is_short = args.iter().any(|arg| arg == "--short");
    let is_long  = args.iter().any(|arg| arg == "--long");

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
    let gs: GamestateResult = read_gamestate();
    let log_epoch       = gs.epoch;
    let initial_month   = gs.initial_month;
    let initial_year    = gs.initial_year;
    let owners_data_after_log = gs.owners_data;
    let owns_data_after_log   = gs.owns_data;
    let remaining_after_log   = gs.remaining;

    // Shared accumulators used in summary modes.
    // wins:       country_id -> win count
    // turns_sum:  total turns across all runs (for avg)
    // turns_sq:   sum of squares of turns (for 95% CI)
    let wins: Arc<Mutex<HashMap<u16, u32>>>  = Arc::new(Mutex::new(HashMap::new()));
    let turns_sum: Arc<Mutex<f64>>           = Arc::new(Mutex::new(0.0));
    let turns_sq:  Arc<Mutex<f64>>           = Arc::new(Mutex::new(0.0));

    let wall_start = Instant::now();

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

        let winner_id = *remaining.iter().next().unwrap();
        let turns_taken = epoch - log_epoch;

        if is_short || is_long {
            let mut w = wins.lock().unwrap();
            *w.entry(winner_id).or_insert(0) += 1;
            drop(w);

            if is_long {
                let t = turns_taken as f64;
                let mut s  = turns_sum.lock().unwrap(); *s += t; drop(s);
                let mut sq = turns_sq.lock().unwrap();  *sq += t * t; drop(sq);
            }
        } else {
            println!("{}", country_data[&winner_id].name);
        }
    });

    if !is_short && !is_long {
        return;
    }

    let wall_elapsed = wall_start.elapsed().as_secs();
    let wins_map = wins.lock().unwrap();

    // Sort by descending win count
    let mut sorted: Vec<(u16, u32)> = wins_map.iter().map(|(&id, &n)| (id, n)).collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let n_runs_f = n_runs as f64;

    if is_short {
        println!("Top winners ({} simulations)", n_runs);
        for (rank, (id, count)) in sorted.iter().take(5).enumerate() {
            let pct = (*count as f64 / n_runs_f) * 100.0;
            let name = &country_data[id].name;
            println!("{}. {} ({} wins, {:.2}%)", rank + 1, name, count, pct);
        }
        return;
    }

    // ── Long format ────────────────────────────────────────────────────────
    let ts = turns_sum.lock().unwrap();
    let tq = turns_sq.lock().unwrap();
    let avg_turns = *ts / n_runs_f;
    // 95% CI half-width: 1.96 * stderr = 1.96 * sqrt(variance / n)
    let variance = (*tq / n_runs_f) - (avg_turns * avg_turns);
    let stderr = (variance / n_runs_f).sqrt();
    let ci_half = 1.96 * stderr;
    let ci_lo = (avg_turns - ci_half).round() as i64;
    let ci_hi = (avg_turns + ci_half).round() as i64;
    let avg_turns_rounded = avg_turns.round() as i64;

    let current_date = epoch_to_date(log_epoch, initial_month, initial_year);
    let gameplay_remaining = months_to_duration_str(avg_turns);

    println!("# Simulation results for {}", current_date);
    println!();
    println!("* **Runs simulated:** {} (in {})", n_runs, format_duration(wall_elapsed));
    println!();
    println!(
        "* **Estimated turns remaining (avg & 95% confidence interval):** {} ({}-{})",
        avg_turns_rounded, ci_lo, ci_hi
    );
    println!();
    println!(
        "* **Estimated gameplay time remaining (avg turns, 1h per turn):** {}",
        gameplay_remaining
    );
    println!();
    println!("## Wins by country:");

    // All countries that ever appear in owners_data (even if already eliminated).
    // We want to list every country, including those with 0 wins.
    // The set of all countries is the set of all owner IDs in the original owners map,
    // plus any territory IDs that could riot back in — but practically, the full set of
    // country IDs is derived from country_data (all entries).
    // Use wins_map keys as "won at least once"; and for all countries, check country_data.
    let mut all_countries: Vec<(u16, u32)> = country_data
        .keys()
        .map(|&id| (id, *wins_map.get(&id).unwrap_or(&0)))
        .collect();
    all_countries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    for (rank, (id, count)) in all_countries.iter().enumerate() {
        let pct = (*count as f64 / n_runs_f) * 100.0;
        let name = &country_data[id].name;
        println!("{}. {} ({}, {:.2}%)", rank + 1, name, count, pct);
    }
}
