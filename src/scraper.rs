use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::fs;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
enum ConquestData {
    Conquer(usize, usize, Vec<usize>),
    Riot(usize, usize),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct SaveFile {
    iteration: usize,
    conquests: (usize, ConquestData),
    countries: Vec<(String, Vec<u16>)>,
    alliances: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct PostFile {
    attacker: String,
    defender: String,
    territory: String,
    #[serde(rename = "type")]
    action_type: String,
    alliances: serde_json::Map<String, serde_json::Value>,
    tags: (String, String),
    pic: String,
    zoom: String,
    caption: String,
    comment: String,
    conquest: ConquestSchema,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CapitulationEvent {
    round: usize,
    #[serde(rename = "attackerTerritoriesBefore")]
    attacker_territories_before: usize,
    #[serde(rename = "defenderTerritoriesBefore")]
    defender_territories_before: usize,
    #[serde(rename = "territoriesCeded")]
    territories_ceded: usize,
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
    capitulation_event: Option<CapitulationEvent>,
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
    country_data: BTreeMap<u16, u16>,
}

struct CountryRow {
    id: String,
    name: String,
    lon: String,
    lat: String,
}

fn month_to_num(m: &str) -> Option<u32> {
    match m {
        "January" => Some(1),
        "February" => Some(2),
        "March" => Some(3),
        "April" => Some(4),
        "May" => Some(5),
        "June" => Some(6),
        "July" => Some(7),
        "August" => Some(8),
        "September" => Some(9),
        "October" => Some(10),
        "November" => Some(11),
        "December" => Some(12),
        _ => None,
    }
}

fn num_to_month(m: u32) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

#[derive(Debug)]
enum FetchError {
    Http(ureq::Error),
    Io(std::io::Error),
    Deserialization {
        endpoint: &'static str,
        body: String,
        other_body: Option<(String, String)>,
        source: serde_json::Error,
    },
}


impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Http(e) => write!(f, "HTTP error: {}", e),
            FetchError::Io(e) => write!(f, "IO error: {}", e),
            FetchError::Deserialization {
                endpoint, source, ..
            } => {
                write!(f, "Deserialization failure for {}: {}", endpoint, source)
            }
        }
    }
}

impl std::error::Error for FetchError {}

impl From<ureq::Error> for FetchError {
    fn from(e: ureq::Error) -> Self {
        FetchError::Http(e)
    }
}

impl From<std::io::Error> for FetchError {
    fn from(e: std::io::Error) -> Self {
        FetchError::Io(e)
    }
}

fn try_fetch_round(
    round: usize,
) -> Result<(SaveFile, PostFile, String, String), FetchError> {
    let save_url = format!("https://run5.worldwarbot.com/data/saves/{:06}.json", round);
    let mut save_res = ureq::get(&save_url).call()?;
    let save_body = save_res.body_mut().read_to_string()?;

    let post_url = format!("https://run5.worldwarbot.com/data/posts/{:06}.json", round);
    let mut post_res = ureq::get(&post_url).call()?;
    let post_body = post_res.body_mut().read_to_string()?;

    // Pretty-print both for helpful error coordinates and context dumping
    let save_val_res: Result<serde_json::Value, _> = serde_json::from_str(&save_body);
    let (save_pretty, save_err) = match save_val_res {
        Ok(v) => (
            serde_json::to_string_pretty(&v).unwrap_or(save_body.clone()),
            None,
        ),
        Err(e) => (save_body.clone(), Some(e)),
    };

    let post_val_res: Result<serde_json::Value, _> = serde_json::from_str(&post_body);
    let (post_pretty, post_err) = match post_val_res {
        Ok(v) => (
            serde_json::to_string_pretty(&v).unwrap_or(post_body.clone()),
            None,
        ),
        Err(e) => (post_body.clone(), Some(e)),
    };

    // Handle raw JSON syntax errors if any
    if let Some(e) = save_err {
        return Err(FetchError::Deserialization {
            endpoint: "save",
            body: save_pretty,
            other_body: Some(("post".to_string(), post_pretty)),
            source: e,
        });
    }
    if let Some(e) = post_err {
        return Err(FetchError::Deserialization {
            endpoint: "post",
            body: post_pretty,
            other_body: Some(("save".to_string(), save_pretty)),
            source: e,
        });
    }

    // Now try schema validation against structs
    let save: SaveFile =
        serde_json::from_str(&save_pretty).map_err(|e| FetchError::Deserialization {
            endpoint: "save",
            body: save_pretty.clone(),
            other_body: Some(("post".to_string(), post_pretty.clone())),
            source: e,
        })?;

    let post: PostFile =
        serde_json::from_str(&post_pretty).map_err(|e| FetchError::Deserialization {
            endpoint: "post",
            body: post_pretty.clone(),
            other_body: Some(("save".to_string(), save_pretty.clone())),
            source: e,
        })?;

    Ok((save, post, save_pretty, post_pretty))
}

pub fn reset_gamestate() -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string("data/country_data.csv")?;
    let mut current_rows = Vec::new();
    let mut original_content = String::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
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

    if let Ok((save, post, _, _)) = fetch_result {
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
            new_content.push_str(&format!(
                "{};{};{};{}\n",
                row.id, row.name, row.lon, row.lat
            ));
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

    let mut country_data = BTreeMap::new();
    for row in &current_rows {
        if !row.id.is_empty() {
            let id = row.id.parse().unwrap();
            country_data.insert(id, id);
        }
    }

    let gamestate = Gamestate {
        epoch: 0,
        initial_month,
        initial_year,
        country_data: country_data.into_iter().collect(),
    };

    fs::write(
        "data/gamestate.json",
        serde_json::to_string_pretty(&gamestate)?,
    )?;

    let targets_req = ureq::get("https://run5.worldwarbot.com/data/voronoi-neighbors.json").call();
    match targets_req {
        Ok(mut res) => {
            let targets_json: String = res.body_mut().read_to_string()?;
            fs::write("data/targets.json", targets_json)?;
            println!("Successfully fetched and updated data/targets.json.");
        }
        Err(e) => {
            eprintln!(
                "Failed to fetch voronoi-neighbors.json: {}. Retaining existing file if present.",
                e
            );
        }
    }

    if generated_from_saves {
        println!(
            "Successfully generated purely synced starting gamestate and cleared logs at epoch 0."
        );
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

pub fn update_gamestate() -> Result<(), Box<dyn std::error::Error>> {
    let mut data_req = ureq::get("https://run5.worldwarbot.com/data/data.json").call()?;
    let data_info: DataInfo = data_req.body_mut().read_json()?;
    let max_iter = data_info.iteration;

    let gamestate_str = fs::read_to_string("data/gamestate.json")?;
    let mut current_state: Gamestate = serde_json::from_str(&gamestate_str)?;
    let targets_data = crate::utils::read_targets_data();
    let country_rows = crate::utils::read_country_data();
    let name_to_id: HashMap<String, u16> = country_rows
        .iter()
        .map(|(&id, c)| (c.name.clone(), id))
        .collect();

    let local_round = current_state.epoch;

    if local_round >= max_iter {
        println!("Gamestate already up to date at round {}.", max_iter);
        return Ok(());
    }

    for round in (local_round + 1)..=max_iter {
        let (save, post, save_pretty, post_pretty) = match try_fetch_round(round) {
            Ok(data) => data,
            Err(e) => {
                match e {
                    FetchError::Deserialization {
                        endpoint,
                        body,
                        other_body,
                        source,
                    } => {
                        eprintln!(
                            "Round {}: Deserialization failure for {}: {}",
                            round, endpoint, source
                        );
                        write_unexpected(endpoint, round, &body);
                        if let Some((other_name, other_data)) = other_body {
                            write_unexpected(&other_name, round, &other_data);
                        }
                    }

                    _ => {
                        eprintln!("Round {}: Failed to fetch data: {}", round, e);
                    }
                }
                std::process::exit(1);
            }
        };

        let mut any_unexpected = false;

        match &save.conquests.1 {
            ConquestData::Conquer(att_t_id, def_t_id, subjects) => {
                if post.action_type != "conquest" {
                    eprintln!("Round {}: Expected post type 'conquest' for Conquer shape, got '{}'", round, post.action_type);
                    any_unexpected = true;
                }
                if post.conquest.action_type != "conquer" {
                    eprintln!("Round {}: Expected conquest.action_type 'conquer' for Conquer shape, got '{}'", round, post.conquest.action_type);
                    any_unexpected = true;
                }
                if *att_t_id != post.conquest.attacker {
                    eprintln!("Round {}: Save attacker territory {} != post attacker territory {}", round, att_t_id, post.conquest.attacker);
                    any_unexpected = true;
                }
                if *def_t_id != post.conquest.defender {
                    eprintln!("Round {}: Save defender territory {} != post defender territory {}", round, def_t_id, post.conquest.defender);
                    any_unexpected = true;
                }
                if subjects.len() != 1 || subjects[0] != *def_t_id {
                    eprintln!("Round {}: Expected subjects [{}] in save Conquer shape, got {:?}", round, def_t_id, subjects);
                    any_unexpected = true;
                }
            }
            ConquestData::Riot(t_id1, t_id2) => {
                if post.action_type != "riot" {
                    eprintln!("Round {}: Expected post type 'riot' for Riot shape, got '{}'", round, post.action_type);
                    any_unexpected = true;
                }
                if post.conquest.action_type != "riot" {
                    eprintln!("Round {}: Expected conquest.action_type 'riot' for Riot shape, got '{}'", round, post.conquest.action_type);
                    any_unexpected = true;
                }
                if t_id1 != t_id2 {
                    eprintln!("Round {}: Riot shape expects identical IDs in save file, got {} and {}", round, t_id1, t_id2);
                    any_unexpected = true;
                }
                if *t_id1 != post.conquest.attacker || *t_id1 != post.conquest.defender {
                    eprintln!("Round {}: Save riot ID {} does not match post attacker/defender {}/{}", round, t_id1, post.conquest.attacker, post.conquest.defender);
                    any_unexpected = true;
                }
                if post.conquest.subjects.len() != 0 {
                    eprintln!("Round {}: Expected zero subjects in post for riot, got {}", round, post.conquest.subjects.len());
                    any_unexpected = true;
                }
            }
        }


        let territory_id = match &save.conquests.1 {
            ConquestData::Conquer(_, def_t_id, _) => *def_t_id,
            ConquestData::Riot(t_id, _) => *t_id,
        };
        let conquered_territory_id = territory_id as u16;
        let id_owners: HashMap<u16, u16> = current_state.country_data.iter().map(|(&k, &v)| (k, v)).collect();

        let (attacker_country_id, defender_country_id) = if post.action_type == "conquest" {
            (name_to_id[&post.attacker], name_to_id[&post.defender])
        } else {
            // Riot case: post.territory is the name of the new independent country.
            // post.attacker is the country it rose against.
            if post.attacker != post.defender {
                eprintln!("Round {}: Expected attacker == defender for riot post, got {} and {}", round, post.attacker, post.defender);
                any_unexpected = true;
            }
            (name_to_id[&post.territory], name_to_id[&post.attacker])
        };

        let attacking_territory_id = post.conquest.attacker as u16;

        // Validations before state change
        if id_owners[&conquered_territory_id] != defender_country_id {
            eprintln!("Round {}: Defender mismatch for territory {}. Expected owner (from API name: {}) {}, got {}", round, conquered_territory_id, post.defender, defender_country_id, id_owners[&conquered_territory_id]);
            any_unexpected = true;
        }

        if post.action_type == "conquest" {
            if post.conquest.capitulation {
                let event = post.conquest.capitulation_event.as_ref().unwrap();
                let ceded_ids: Vec<u16> = post
                    .conquest
                    .subjects
                    .iter()
                    .map(|v| {
                        v.as_u64().map(|id| id as u16).unwrap_or_else(|| {
                            v.as_str().and_then(|s| s.parse().ok()).unwrap_or_default()
                        })
                    })
                    .collect();

                if ceded_ids.len() != event.territories_ceded {
                    eprintln!(
                        "Round {}: Capitulation subjects count {} != ceded count {}",
                        round,
                        ceded_ids.len(),
                        event.territories_ceded
                    );
                    any_unexpected = true;
                }

                if let Err(e) = crate::game_utils::validate_capitulation(
                    post.conquest.defender as u16,
                    &ceded_ids,
                    &id_owners,
                    &targets_data,
                ) {
                    eprintln!("Round {}: Capitulation validation failed: {}", round, e);
                    any_unexpected = true;
                }
            } else {
                if post.conquest.subjects.len() != 0 {
                    eprintln!(
                        "Round {}: Expected zero subjects in post for non-capitulating conquest, got {}",
                        round,
                        post.conquest.subjects.len()
                    );
                    any_unexpected = true;
                }
            }
            if id_owners[&attacking_territory_id] != attacker_country_id {
                eprintln!(
                    "Round {}: Attacker {} ({}) does not own the launching territory {} (owned by {}).",
                    round,
                    post.attacker,
                    attacker_country_id,
                    attacking_territory_id,
                    id_owners[&attacking_territory_id]
                );
                any_unexpected = true;
            }
            if !crate::game_utils::find_attack_targets(
                attacking_territory_id,
                &id_owners,
                &targets_data,
            )
            .contains(&conquered_territory_id)
            {
                eprintln!("Round {}: Launching territory {} could not have reached territory {} based on graph logic.", round, attacking_territory_id, conquered_territory_id);
                any_unexpected = true;
            }
        }

        let defender_territories_before = current_state
            .country_data
            .values()
            .filter(|&owner| *owner == defender_country_id)
            .count();
        let completely_defeated = defender_territories_before == 1;

        // Apply state change
        current_state
            .country_data
            .insert(conquered_territory_id, attacker_country_id);
        if post.action_type == "conquest" {
            for sub_val in &post.conquest.subjects {
                let sub_id = sub_val.as_u64().map(|v| v as u16).unwrap_or_else(|| {
                    sub_val.as_str().and_then(|s| s.parse().ok()).unwrap_or(0)
                });
                current_state.country_data.insert(sub_id, attacker_country_id);
            }
        }

        current_state.epoch = round;

        let remaining_count = current_state
            .country_data
            .values()
            .collect::<std::collections::HashSet<_>>()
            .len();
        let total_months =
            current_state.initial_year * 12 + (current_state.initial_month - 1) + (round as u32);

        let mut d_string = String::new();
        if id_owners[&conquered_territory_id] != conquered_territory_id {
            d_string.push_str(&format!(" previously occupied by {}", post.defender));
        }

        if completely_defeated {
            d_string.push_str(&format!(
                ".\n{} has been completely defeated.\n{_e} countries remaining.",
                post.defender,
                _e = remaining_count
            ));
        } else if post.conquest.capitulation {
            let event = post.conquest.capitulation_event.as_ref().unwrap();
            let ceded = event.territories_ceded;
            d_string.push_str(&format!(
                ".\n{} capitulated, ceding {} additional territor{} to {}.",
                post.defender,
                ceded,
                if ceded == 1 { "y" } else { "ies" },
                post.attacker
            ));
        } else if post.conquest.fallen_capital_remnant {
            d_string.push_str(&format!(
                ".\nThe government of {} continues in exile, based in its remaining territories.",
                post.defender
            ));
        } else {
            d_string.push('.');
        }

        let date_prefix = format!(
            "{} {}, ",
            num_to_month((total_months % 12) + 1),
            total_months / 12
        );

        let country_exists = current_state
            .country_data
            .values()
            .any(|&o| o == attacker_country_id);

        let event_text = if post.action_type == "conquest" {
            format!(
                "{} conquered {} territory{}",
                post.attacker, post.territory, d_string
            )
        } else {
            let riot_suffix = if country_exists {
                "reunited its homeland."
            } else {
                "gained independence."
            };
            format!(
                "{} rose against {} and {}",
                post.territory, post.attacker, riot_suffix
            )
        };

        let expected_caption = format!(
            "{date_prefix}{event_text}\nCheck the full map at https://worldwarbot.com"
        );

        if post.caption != expected_caption {
            eprintln!(
                "Round {}: Caption validation failed!\nExpected: {}\nGot     : {}",
                round, expected_caption, post.caption
            );
            any_unexpected = true;
        }
        if post.pic != format!("{:06}.png", round)
            || post.zoom != format!("{:06}.png", round)
            || !post.alliances.is_empty()
        {
            eprintln!("Round {}: post pic, zoom, or alliances mismatch.", round);
            any_unexpected = true;
        }

        // Grouped SaveFile validation
        if save.iteration != round || save.conquests.0 != round || !save.alliances.is_empty() {
            eprintln!(
                "Round {}: save iteration, conquest round, or alliances mismatch.",
                round,
            );
            any_unexpected = true;
        }
        let mut groups: HashMap<u16, Vec<u16>> = HashMap::new();
        for (&t_id, &o_id) in &current_state.country_data {
            groups.entry(o_id).or_default().push(t_id);
        }
        let mut mid_list: Vec<(u16, Vec<u16>)> = groups.into_iter().collect();
        for (_, t_ids) in &mut mid_list {
            t_ids.sort();
        }
        mid_list.sort_by_key(|(_, t_ids)| t_ids[0]);

        let expected_save_list: Vec<(String, Vec<u16>)> = mid_list
            .into_iter()
            .map(|(o_id, t_ids)| (country_rows[&o_id].name.clone(), t_ids))
            .collect();

        if save.countries != expected_save_list {
            eprintln!("Round {}: save countries list mismatch.", round);
            any_unexpected = true;
        }

        if any_unexpected {
            write_unexpected("save", round, &save_pretty);
            write_unexpected("post", round, &post_pretty);
            eprintln!("Stopping simulation because validation mismatches were found.");
            std::process::exit(1);
        }

        let mut lines: Vec<&str> = post.caption.lines().collect();
        if lines.len() >= 1 {
            lines.truncate(lines.len() - 1);
        }
        println!("Round {}: {}", round, lines.join(" "));

        fs::write(
            "data/gamestate.json",
            serde_json::to_string_pretty(&current_state)?,
        )?;
    }

    let n_processed = max_iter - local_round;
    if n_processed == 1 {
        println!("Processed 1 new round (round {}).", max_iter);
    } else {
        println!(
            "Processed {} new rounds (rounds {} to {}).",
            n_processed,
            local_round + 1,
            max_iter
        );
    }

    Ok(())
}
