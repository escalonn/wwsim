use std::collections::HashMap;
use std::fs;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use chrono::Datelike;

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct SaveFile {
    iteration: usize,
    conquests: (usize, (usize, usize, Vec<usize>)),
    countries: Vec<(String, Vec<usize>)>,
    alliances: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct PostFile {
    attacker: String,
    defender: String,
    territory: String,
    #[serde(rename = "type")]
    action_type: String,
    alliances: HashMap<String, serde_json::Value>,
    tags: (String, String),
    pic: String,
    zoom: String,
    caption: String,
    comment: String,
    conquest: ConquestSchema,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ConquestSchema {
    attacker: usize,
    defender: usize,
    #[serde(rename = "type")]
    action_type: String,
    subjects: Vec<serde_json::Value>,
    capitulation: bool,
    #[serde(rename = "capitulationEvent")]
    capitulation_event: Option<serde_json::Value>,
    #[serde(rename = "fallenCapitalRemnant")]
    fallen_capital_remnant: bool,
    #[serde(rename = "defenderAdminBefore")]
    defender_admin_before: String,
}

#[derive(Serialize, Deserialize)]
struct Gamestate {
    epoch: usize,
    initial_month: u32,
    initial_year: u32,
    country_data: HashMap<String, String>,
}

struct CountryRow {
    id: String,
    name: String,
    lon: String,
    lat: String,
}

fn month_to_num(m: &str) -> Option<u32> {
    match m {
        "January" => Some(1), "February" => Some(2), "March" => Some(3),
        "April" => Some(4), "May" => Some(5), "June" => Some(6),
        "July" => Some(7), "August" => Some(8), "September" => Some(9),
        "October" => Some(10), "November" => Some(11), "December" => Some(12),
        _ => None,
    }
}

fn num_to_month(m: u32) -> &'static str {
    match m {
        1 => "January", 2 => "February", 3 => "March", 4 => "April", 5 => "May",
        6 => "June", 7 => "July", 8 => "August", 9 => "September", 10 => "October",
        11 => "November", 12 => "December", _ => "",
    }
}

fn try_fetch_round(round: usize) -> Result<(SaveFile, PostFile), Box<dyn std::error::Error>> {
    let save_url = format!("https://run5.worldwarbot.com/data/saves/{:06}.json", round);
    let save_req = ureq::get(&save_url).call()?;
    let save_str = save_req.into_string()?;

    let post_url = format!("https://run5.worldwarbot.com/data/posts/{:06}.json", round);
    let post_req = ureq::get(&post_url).call()?;
    let post_str = post_req.into_string()?;

    let save: SaveFile = serde_json::from_str(&save_str)?;
    let post: PostFile = serde_json::from_str(&post_str)?;

    Ok((save, post))
}

pub fn reset_gamestate() -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string("data/country_data.csv")?;
    let mut current_rows = Vec::new();
    let mut original_content = String::new();

    for line in content.lines() {
        if line.trim().is_empty() { continue; }
        original_content.push_str(line.trim());
        original_content.push('\n');

        let parts: Vec<&str> = line.split(';').collect();
        current_rows.push(CountryRow {
            id: parts[0].to_string(),
            name: parts[1].to_string(),
            lon: parts.get(2).unwrap_or(&"").to_string(),
            lat: parts.get(3).unwrap_or(&"").to_string(),
        });
    }

    let mut id_map: HashMap<String, String> = HashMap::new();
    let fetch_result = try_fetch_round(1);

    let mut generated_from_saves = false;
    let mut initial_month = chrono::Utc::now().month();
    let mut initial_year = chrono::Utc::now().year() as u32;

    if let Ok((save, post)) = fetch_result {
        println!("Successfully fetched Round 1. Updating IDs.");
        generated_from_saves = true;

        let parts: Vec<&str> = post.caption.split(' ').collect();
        if parts.len() >= 2 {
            if let Some(m) = month_to_num(parts[0]) {
                let y: u32 = parts[1].trim_end_matches(',').parse().unwrap_or(2026);
                if m == 1 {
                    initial_month = 12;
                    initial_year = y - 1;
                } else {
                    initial_month = m - 1;
                    initial_year = y;
                }
            }
        }

        for (name, terrs) in &save.countries {
            if terrs.len() == 1 {
                id_map.insert(name.clone(), terrs[0].to_string());
            }
        }
        id_map.insert(post.attacker.clone(), post.conquest.attacker.to_string());
        id_map.insert(post.defender.clone(), post.conquest.defender.to_string());

        let mut matched_names = std::collections::HashSet::new();

        for row in &mut current_rows {
            if let Some(bot_id) = id_map.get(&row.name) {
                row.id = bot_id.clone();
                matched_names.insert(row.name.clone());
            } else {
                row.id = String::new();
            }
        }

        for (name, bot_id) in &id_map {
            if !matched_names.contains(name) {
                current_rows.push(CountryRow {
                    id: bot_id.clone(),
                    name: name.clone(),
                    lon: String::new(),
                    lat: String::new(),
                });
            }
        }

        current_rows.sort_by(|a, b| {
            if a.id.is_empty() && !b.id.is_empty() {
                Ordering::Greater
            } else if !a.id.is_empty() && b.id.is_empty() {
                Ordering::Less
            } else if a.id.is_empty() && b.id.is_empty() {
                a.name.cmp(&b.name)
            } else {
                let id_a = a.id.parse::<usize>().unwrap();
                let id_b = b.id.parse::<usize>().unwrap();
                id_a.cmp(&id_b)
            }
        });

        let mut new_content = String::new();
        for row in &current_rows {
            new_content.push_str(&format!("{};{};{};{}\n", row.id, row.name, row.lon, row.lat));
        }

        if new_content != original_content {
            fs::write("data/country_data.csv", new_content)?;
            println!("Updated data/country_data.csv with corrected IDs based on Round 1 data.");
        } else {
            println!("data/country_data.csv mapped cleanly without modifications.");
        }
    } else {
        println!("Failed to retrieve Round 1 data. Retaining existing IDs directly from country_data.csv.");
    }

    let mut country_data = HashMap::new();
    for row in &current_rows {
        if !row.id.is_empty() {
            country_data.insert(row.id.clone(), row.id.clone());
        }
    }

    let gamestate = Gamestate {
        epoch: 0,
        initial_month,
        initial_year,
        country_data,
    };

    fs::write("data/gamestate.json", serde_json::to_string_pretty(&gamestate)?)?;
    fs::write("data/log.csv", "")?;

    if generated_from_saves {
        println!("Successfully generated purely synced starting gamestate and cleared logs at epoch 0.");
    }

    Ok(())
}

fn write_unexpected(endpoint: &str, round: usize, data: &str) {
    let filename = format!("data/unexpected_{}_{:06}.json", endpoint, round);
    fs::write(&filename, data).unwrap_or_default();
    eprintln!("Dumped unexpected data to {}", filename);
}

#[derive(Deserialize, Debug)]
struct DataInfo {
    iteration: usize,
}

pub fn update_gamestate(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let data_req = ureq::get("https://run5.worldwarbot.com/data/data.json").call()?;
    let data_str = data_req.into_string()?;
    let data_info: DataInfo = serde_json::from_str(&data_str)?;
    let max_iter = data_info.iteration;

    let gamestate_str = fs::read_to_string("data/gamestate.json")?;
    let mut current_state: Gamestate = serde_json::from_str(&gamestate_str)?;

    let local_round = current_state.epoch;

    if local_round >= max_iter {
        return Ok(());
    }

    for round in (local_round + 1)..=max_iter {
        let fetch_result = try_fetch_round(round);
        if fetch_result.is_err() {
            if !force {
                eprintln!("Could not retrieve round {} data", round);
                break;
            } else {
                eprintln!("Skipping error on round {}", round);
                continue;
            }
        }

        let (save, post) = fetch_result.unwrap();
        let mut any_unexpected = false;

        if !save.alliances.is_empty() || !post.alliances.is_empty() {
            eprintln!("Round {}: Alliances strictly expected to be empty.", round);
            any_unexpected = true;
        }

        if post.action_type != "conquest" {
            eprintln!("Round {}: Expected event_type 'conquest'", round);
            any_unexpected = true;
        }

        if post.conquest.action_type != "conquer" {
            eprintln!("Round {}: Expected conquest.action_type 'conquer'", round);
            any_unexpected = true;
        }

        if post.conquest.subjects.len() != 0 {
            eprintln!("Round {}: Expected conquest.subjects to natively be empty.", round);
            any_unexpected = true;
        }

        let territory_id = save.conquests.1.1;

        let defender_territories = current_state.country_data.values().filter(|&owner| owner == &post.conquest.defender.to_string()).count();
        let completely_defeated = defender_territories == 1;

        let total_months = current_state.initial_year * 12 + (current_state.initial_month - 1) + (round as u32);
        let current_round_year = total_months / 12;
        let current_round_month = (total_months % 12) + 1;

        current_state.country_data.insert(territory_id.to_string(), post.conquest.attacker.to_string());

        let remaining_keys: std::collections::HashSet<_> = current_state.country_data.values().collect();
        let remaining_count = remaining_keys.len();

        let d_string = if completely_defeated {
            format!("{} has been completely defeated.\n", post.defender)
        } else {
            "".to_string()
        };

        let expected_caption = format!("{} {}, {} conquered {} territory.\n{}{_e} countries remaining.\nCheck the full map at https://worldwarbot.com", num_to_month(current_round_month), current_round_year, post.attacker, post.territory, d_string, _e = remaining_count);

        if post.caption != expected_caption {
            eprintln!("Round {}: Caption validation failed! Expected exactly:\n'{}'\nGot:\n'{}'", round, expected_caption, post.caption);
            any_unexpected = true;
        }

        if post.pic != format!("{:06}.png", round) || post.zoom != format!("{:06}.png", round) {
            eprintln!("Round {}: Picture format strings are not matching round info.", round);
            any_unexpected = true;
        }

        if any_unexpected && !force {
            let save_str = serde_json::to_string(&save)?;
            let post_str = serde_json::to_string(&post)?;
            write_unexpected("save", round, &save_str);
            write_unexpected("post", round, &post_str);
            eprintln!("Stopping simulation because validation mismatches were found.");
            std::process::exit(1);
        } else if any_unexpected && force {
            eprintln!("Validation issues encountered in Round {}, --force flag bypassed it.", round);
        }

        current_state.epoch = round;
        let log_line = format!("{};{};{}\n", post.conquest.action_type, post.conquest.attacker, post.conquest.defender);

        let mut lines: Vec<&str> = post.caption.lines().collect();
        if lines.len() >= 2 {
            lines.truncate(lines.len() - 2);
        }
        let printed_caption = lines.join(" ");
        println!("Round {}: {}", round, printed_caption);

        fs::write("data/gamestate.json", serde_json::to_string_pretty(&current_state)?)?;
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open("data/log.csv") {
            use std::io::Write;
            let _ = file.write_all(log_line.as_bytes());
        }
    }

    Ok(())
}
