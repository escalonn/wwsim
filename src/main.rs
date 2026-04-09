use rand::random;
use rayon::prelude::*;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use indicatif::{ProgressBar, ProgressStyle};
use plotters::prelude::*;

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

fn turns_to_duration_str(avg_turns: f64) -> String {
    // 1 turn ≈ 1 hour of gameplay
    let days = (avg_turns / 24.0) as u64;
    let hours = (avg_turns % 24.0) as u64;
    let mut parts = Vec::new();
    if days > 0  { parts.push(format!("{} day{}", days, if days == 1 { "" } else { "s" })); }
    if hours > 0 { parts.push(format!("{} hour{}", hours, if hours == 1 { "" } else { "s" })); }
    if parts.is_empty() { parts.push("less than 1 hour".to_string()); }
    parts.join(", ")
}

///////////////////////////////////////////////////////////////////////////////
// Chart generation

fn generate_chart(
    sorted: &[(u16, u32)],
    country_data: &HashMap<u16, Country>,
    n_runs: usize,
    epoch: usize,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Show only countries that actually won at least once, up to 20.
    let bars: Vec<(String, f64)> = sorted
        .iter()
        .take_while(|(_, c)| *c > 0)
        .take(20)
        .map(|(id, count)| {
            let pct = (*count as f64 / n_runs as f64) * 100.0;
            let name: String = country_data[id].name.chars().take(32).collect();
            (name, pct)
        })
        .collect();

    if bars.is_empty() {
        return Ok(());
    }

    let n = bars.len() as i32;
    let bar_h      = 36i32;
    let gap        = 8i32;
    let margin_top = 62i32;
    let margin_l   = 230i32; // country names column
    let margin_r   = 80i32;  // space for pct labels
    let margin_bot = 20i32;
    let img_w      = 1040u32;
    let img_h      = (margin_top + (bar_h + gap) * n + margin_bot) as u32;
    let bar_area_w = img_w as i32 - margin_l - margin_r;

    let root = BitMapBackend::new(path, (img_w, img_h)).into_drawing_area();
    root.fill(&RGBColor(18, 18, 30))?;

    let max_pct = bars.iter().map(|(_, p)| *p).fold(1.0f64, f64::max);

    // Title
    root.draw(&Text::new(
        format!("Round {} — {} simulations", epoch, n_runs),
        (img_w as i32 / 2 - 140, 18),
        ("Arial", 20).into_font().color(&RGBColor(230, 230, 240)),
    ))?;

    for (i, (name, pct)) in bars.iter().enumerate() {
        let i32i = i as i32;
        let y = margin_top + (bar_h + gap) * i32i;
        let bar_px = ((pct / max_pct) * bar_area_w as f64) as i32;

        // Background track
        root.draw(&Rectangle::new(
            [(margin_l, y + 3), (margin_l + bar_area_w, y + bar_h - 3)],
            RGBColor(32, 32, 48).filled(),
        ))?;

        // Colored bar — gradient teal → purple
        let t = if bars.len() > 1 { i as f64 / (bars.len() - 1) as f64 } else { 0.0 };
        let color = RGBColor(
            (78.0  + t * 100.0) as u8,
            (205.0 - t * 120.0) as u8,
            (196.0 + t * 20.0)  as u8,
        );
        root.draw(&Rectangle::new(
            [(margin_l, y + 3), (margin_l + bar_px.max(2), y + bar_h - 3)],
            color.filled(),
        ))?;

        // Rank number
        root.draw(&Text::new(
            format!("{}.", i + 1),
            (margin_l - 36, y + 9),
            ("Arial", 12).into_font().color(&RGBColor(130, 130, 160)),
        ))?;

        // Country name
        root.draw(&Text::new(
            name.as_str(),
            (4, y + 9),
            ("Arial", 13).into_font().color(&RGBColor(215, 215, 230)),
        ))?;

        // Percentage label
        root.draw(&Text::new(
            format!("{:.2}%", pct),
            (margin_l + bar_px + 6, y + 9),
            ("Arial", 13).into_font().color(&RGBColor(240, 240, 255)),
        ))?;
    }

    root.present()?;
    Ok(())
}

///////////////////////////////////////////////////////////////////////////////

fn main() {
    let args: Vec<String> = env::args().collect();
    let is_reset   = args.iter().any(|arg| arg == "--reset");
    let is_local   = args.iter().any(|arg| arg == "--local");
    let is_verbose = args.iter().any(|arg| arg == "--verbose");

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
    let GamestateResult {
        owners_data: owners_data_after_log,
        owns_data: owns_data_after_log,
        remaining: remaining_after_log,
        epoch: log_epoch,
        initial_month,
        initial_year,
    } = read_gamestate();

    // Shared accumulators (not used in --verbose mode but always created cheaply)
    let wins:      Arc<Mutex<HashMap<u16, u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let turns_sum: Arc<Mutex<f64>>               = Arc::new(Mutex::new(0.0));
    let turns_sq:  Arc<Mutex<f64>>               = Arc::new(Mutex::new(0.0));

    // Progress bar (only shown in default/summary mode)
    let pb = if !is_verbose {
        let bar = ProgressBar::new(n_runs as u64);
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.cyan} [{elapsed_precise}] [{bar:45.cyan/blue}] {pos}/{len} runs  (eta {eta})"
            )
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "),
        );
        bar
    } else {
        ProgressBar::hidden()
    };

    let wall_start = Instant::now();

    // Each run is independent; rayon parallelises across available threads.
    (0..n_runs).into_par_iter().for_each(|_| {
        let mut epoch = log_epoch;

        let mut remaining   = remaining_after_log.clone();
        let mut owners_data = owners_data_after_log.clone();
        let mut owns_data   = owns_data_after_log.clone();

        let owners_ref    = &mut owners_data;
        let owns_ref      = &mut owns_data;
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

        let winner_id   = *remaining.iter().next().unwrap();
        let turns_taken = (epoch - log_epoch) as f64;

        if is_verbose {
            println!("{}", country_data[&winner_id].name);
        } else {
            { let mut w = wins.lock().unwrap(); *w.entry(winner_id).or_insert(0) += 1; }
            { let mut s = turns_sum.lock().unwrap(); *s += turns_taken; }
            { let mut q = turns_sq.lock().unwrap();  *q += turns_taken * turns_taken; }
            pb.inc(1);
        }
    });

    pb.finish_and_clear();

    if is_verbose {
        return;
    }

    let wall_elapsed = wall_start.elapsed().as_secs();
    let wins_map = wins.lock().unwrap();

    // Sort by descending wins, then by id for stable ordering of ties
    let mut sorted: Vec<(u16, u32)> = wins_map.iter().map(|(&id, &n)| (id, n)).collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let n_runs_f = n_runs as f64;

    // ── Short format → stdout ─────────────────────────────────────────────
    println!("Top winners ({} simulations)", n_runs);
    for (rank, (id, count)) in sorted.iter().take(5).enumerate() {
        let pct = (*count as f64 / n_runs_f) * 100.0;
        println!("{}. {} ({} wins, {:.2}%)", rank + 1, &country_data[id].name, count, pct);
    }

    // ── Long format + chart → logs/ ───────────────────────────────────────
    let ts = turns_sum.lock().unwrap();
    let tq = turns_sq.lock().unwrap();
    let avg_turns = *ts / n_runs_f;
    // 95% CI: ±1.96 * stderr
    let variance  = (*tq / n_runs_f) - (avg_turns * avg_turns);
    let ci_half   = 1.96 * (variance / n_runs_f).sqrt();
    let ci_lo     = (avg_turns - ci_half).round() as i64;
    let ci_hi     = (avg_turns + ci_half).round() as i64;

    let current_date       = epoch_to_date(log_epoch, initial_month, initial_year);
    let gameplay_remaining = turns_to_duration_str(avg_turns);

    fs::create_dir_all("logs").expect("Failed to create logs/ directory");
    let log_path   = format!("logs/log_{:06}.md",   log_epoch);
    let chart_path = format!("logs/chart_{:06}.png", log_epoch);

    // Generate chart
    match generate_chart(&sorted, &country_data, n_runs, log_epoch, &chart_path) {
        Ok(()) => println!("\nChart saved to {}", chart_path),
        Err(e) => eprintln!("Warning: chart generation failed: {}", e),
    }

    // Build all-country list (include zeros so every country appears)
    let mut all_countries: Vec<(u16, u32)> = country_data
        .keys()
        .map(|&id| (id, *wins_map.get(&id).unwrap_or(&0)))
        .collect();
    all_countries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut md = String::new();
    md.push_str(&format!("# Simulation results for {}\n\n", current_date));
    md.push_str(&format!(
        "* **Runs simulated:** {} (in {})\n\n",
        n_runs, format_duration(wall_elapsed)
    ));
    md.push_str(&format!(
        "* **Estimated turns remaining (avg & 95% confidence interval):** {} ({}-{})\n\n",
        avg_turns.round() as i64, ci_lo, ci_hi
    ));
    md.push_str(&format!(
        "* **Estimated gameplay time remaining (avg turns, 1h per turn):** {}\n\n",
        gameplay_remaining
    ));
    md.push_str("## Wins by country:\n");
    for (rank, (id, count)) in all_countries.iter().enumerate() {
        let pct = (*count as f64 / n_runs_f) * 100.0;
        md.push_str(&format!(
            "{}. {} ({}, {:.2}%)\n",
            rank + 1, &country_data[id].name, count, pct
        ));
    }

    fs::write(&log_path, &md).expect("Failed to write log file");
    println!("Full results written to {}", log_path);
}
