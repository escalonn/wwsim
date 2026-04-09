use rand::random;
use rayon::prelude::*;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use indicatif::{ProgressBar, ProgressStyle};
use plotters::prelude::*;
use plotters::element::PathElement;

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
// Chart generation — pie chart

/// HSL → RGB (all values in [0, 1] except h which is [0, 360))
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> RGBColor {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    RGBColor(
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

/// Deterministic color from country ID — consistent across runs.
fn country_color(id: u16) -> RGBColor {
    // Knuth multiplicative hash for good bit distribution
    let h = (id as u32).wrapping_mul(2_654_435_761);
    let hue = (h >> 16) as f64 / 65535.0 * 360.0;
    let sat = 0.58 + ((h & 0x3F) as f64 / 63.0) * 0.32;        // 0.58 – 0.90
    let lit = 0.40 + (((h >> 6) & 0x3F) as f64 / 63.0) * 0.22; // 0.40 – 0.62
    hsl_to_rgb(hue, sat, lit)
}

/// Polygon points for a pie slice: center + arc from a0 to a1 (screen coords, y-down).
/// a0 / a1 are in radians; positive = counter-clockwise on screen (since y is flipped).
fn pie_points(cx: i32, cy: i32, r: i32, a0: f64, a1: f64) -> Vec<(i32, i32)> {
    let steps = ((a1 - a0).abs() * r as f64 / 1.5).ceil() as usize;
    let steps = steps.max(4);
    let mut pts = vec![(cx, cy)];
    for i in 0..=steps {
        let a = a0 + (a1 - a0) * i as f64 / steps as f64;
        pts.push((
            cx + (r as f64 * a.cos()) as i32,
            cy + (r as f64 * a.sin()) as i32,
        ));
    }
    pts
}

fn generate_chart(
    sorted: &[(u16, u32)],
    country_data: &HashMap<u16, Country>,
    n_runs: usize,
    log_epoch: usize,
    initial_month: u32,
    initial_year: i32,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    struct Slice { name: String, count: u32, id: Option<u16> }

    // Top 12 named slices + "Other" bucket for the rest.
    const MAX_NAMED: usize = 12;
    let mut slices: Vec<Slice> = Vec::new();
    let mut other_count: u32 = 0;
    for (id, count) in sorted.iter() {
        if *count == 0 { break; }
        if slices.len() < MAX_NAMED {
            slices.push(Slice { name: country_data[id].name.clone(), count: *count, id: Some(*id) });
        } else {
            other_count += count;
        }
    }
    if other_count > 0 {
        slices.push(Slice { name: "Other".to_string(), count: other_count, id: None });
    }
    if slices.is_empty() { return Ok(()); }

    // ── Layout constants ──────────────────────────────────────────────────
    let img_w: u32 = 1000;
    let img_h: u32 =  680;
    let bg = RGBColor(18, 18, 30);

    // Pie is left-of-center; legend on the right.
    let cx = 300i32;
    let cy = 390i32;
    let r  = 260i32;

    let leg_x    = 620i32;  // legend left edge
    let leg_y0   = 80i32;
    let item_h   = 40i32;
    let swatch   = 24i32;

    let root = BitMapBackend::new(path, (img_w, img_h)).into_drawing_area();
    root.fill(&bg)?;

    // ── Title ─────────────────────────────────────────────────────────────
    let date_str = epoch_to_date(log_epoch, initial_month, initial_year);
    root.draw(&Text::new(
        format!("{} — {} simulations", date_str, n_runs),
        (40, 22),
        ("Arial", 28).into_font().color(&RGBColor(230, 230, 248)),
    ))?;

    // ── Pie slices ────────────────────────────────────────────────────────
    // Start at the top (angle = −π/2 in screen coords where y points down ==
    // 3π/2, but we use −π/2 directly since cos/sin handle it correctly).
    // Clockwise = increasing angle in screen coords.
    let sep_color = RGBColor(18, 18, 30);
    let mut angle = -std::f64::consts::FRAC_PI_2;

    for slice in &slices {
        let sweep = slice.count as f64 / n_runs as f64 * std::f64::consts::TAU;
        let end_angle = angle + sweep;
        let color = slice.id.map(country_color).unwrap_or(RGBColor(85, 85, 108));

        // Filled polygon
        root.draw(&Polygon::new(pie_points(cx, cy, r, angle, end_angle), color.filled()))?;

        // Separator spoke at start of this slice
        let sx = cx + (r as f64 * angle.cos()) as i32;
        let sy = cy + (r as f64 * angle.sin()) as i32;
        root.draw(&PathElement::new(
            vec![(cx, cy), (sx, sy)],
            ShapeStyle { color: sep_color.to_rgba(), filled: false, stroke_width: 2 },
        ))?;

        angle = end_angle;
    }
    // Final spoke to close the circle
    let sx = cx + (r as f64 * angle.cos()) as i32;
    let sy = cy + (r as f64 * angle.sin()) as i32;
    root.draw(&PathElement::new(
        vec![(cx, cy), (sx, sy)],
        ShapeStyle { color: sep_color.to_rgba(), filled: false, stroke_width: 2 },
    ))?;

    // Thin circle outline to clean up antialiasing at the perimeter
    root.draw(&Circle::new(
        (cx, cy), r,
        ShapeStyle { color: sep_color.to_rgba(), filled: false, stroke_width: 2 },
    ))?;

    // ── Legend ────────────────────────────────────────────────────────────
    for (i, slice) in slices.iter().enumerate() {
        let y = leg_y0 + i as i32 * item_h;
        let color = slice.id.map(country_color).unwrap_or(RGBColor(85, 85, 108));

        // Color swatch with a subtle dark border
        root.draw(&Rectangle::new(
            [(leg_x, y), (leg_x + swatch, y + swatch)],
            color.filled(),
        ))?;
        root.draw(&Rectangle::new(
            [(leg_x, y), (leg_x + swatch, y + swatch)],
            ShapeStyle { color: RGBColor(50, 50, 70).to_rgba(), filled: false, stroke_width: 1 },
        ))?;

        // Country name + percentage
        let pct = slice.count as f64 / n_runs as f64 * 100.0;
        let name: String = slice.name.chars().take(28).collect();
        root.draw(&Text::new(
            format!("{} — {:.1}%", name, pct),
            (leg_x + swatch + 10, y + 4),
            ("Arial", 18).into_font().color(&RGBColor(215, 215, 232)),
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

    if n_runs > 0 {
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
        match generate_chart(&sorted, &country_data, n_runs, log_epoch, initial_month, initial_year, &chart_path) {
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
}
