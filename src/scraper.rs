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
    /// Parsed separately per-round via `parse_conquest_schema` to allow strict per-round validation.
    conquest: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CapitalsFile {
    iteration: usize,
    #[serde(rename = "originalCapitalByAdmin")]
    original_capital_by_admin: HashMap<String, u16>,
    #[serde(rename = "currentCapitalByAdmin")]
    current_capital_by_admin: HashMap<String, u16>,
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

/// Conquest sub-object for rounds 1–228: uses `fallenCapitalRemnant`, no capital-tracking fields.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ConquestSchemaPre229 {
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

/// Conquest sub-object for round 229 (transitional): still uses `fallenCapitalRemnant`,
/// but adds `capitalIndexAfter` and `defenderCapitalTerritoryAfter`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ConquestSchema229 {
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
    /// Always present (even for eliminations); equals original capital ID when eliminated.
    #[serde(rename = "capitalIndexAfter")]
    capital_index_after: usize,
    /// Name of `capital_index_after` territory; null when defender is eliminated OR capital did not change.
    #[serde(rename = "defenderCapitalTerritoryAfter")]
    defender_capital_territory_after: Option<String>,
}

/// Conquest sub-object for round 230+: renames `fallenCapital` and adds several new fields.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ConquestSchema230 {
    attacker: usize,
    defender: usize,
    #[serde(rename = "type")]
    action_type: String,
    subjects: Vec<serde_json::Value>,
    capitulation: bool,
    #[serde(rename = "capitulationEvent")]
    capitulation_event: Option<CapitulationEvent>,
    /// Renamed from `fallenCapitalRemnant`. Same semantics: true iff capital taken AND
    /// defender survives.
    #[serde(rename = "fallenCapital")]
    fallen_capital: bool,
    #[serde(rename = "defenderAdminBefore")]
    defender_admin_before: String,
    /// Always null (validated). Present as a placeholder for future use.
    #[serde(rename = "defenderTerritoryRankBefore")]
    defender_territory_rank_before: Option<usize>,
    /// Defender's capital territory ID before this conquest.
    #[serde(rename = "capitalIndexBefore")]
    capital_index_before: usize,
    /// Defender's capital territory ID after this conquest (original when eliminated).
    #[serde(rename = "capitalIndexAfter")]
    capital_index_after: usize,
    /// Name of `capital_index_after`; null when defender is eliminated OR capital did not change.
    #[serde(rename = "defenderCapitalTerritoryAfter")]
    defender_capital_territory_after: Option<String>,
    /// Defender's original (never-relocated) capital ID. Must always equal defender country ID.
    #[serde(rename = "originalCapitalIndex")]
    original_capital_index: usize,
    /// True iff `capitalIndexBefore != capitalIndexAfter` (i.e., capital moved this round).
    #[serde(rename = "capitalMoved")]
    capital_moved: bool,
}

/// Normalized conquest information, common across all schema versions.
struct ConquestInfo {
    attacker: usize,
    defender: usize,
    action_type: String,
    subjects: Vec<serde_json::Value>,
    capitulation: bool,
    capitulation_event: Option<CapitulationEvent>,
    fallen_capital_remnant: bool,
    defender_admin_before: String,
    /// Round 229+: defender's capital territory ID after this conquest.
    capital_index_after: Option<usize>,
    /// Round 229+: name of `capital_index_after`; None when defender is eliminated.
    defender_capital_territory_after: Option<String>,
    /// Round 230+: defender's capital territory ID before this conquest.
    capital_index_before: Option<usize>,
    /// Round 230+: defender's original (never-relocated) capital ID.
    original_capital_index: Option<usize>,
    /// Round 230+: true iff the capital moved this round.
    capital_moved: Option<bool>,
    // `defenderTerritoryRankBefore` (round 230+) is validated to be always None and not carried through.
}

/// Parse and strictly validate the `conquest` JSON object for the given round,
/// returning a normalized [`ConquestInfo`] on success or an error string on failure.
fn parse_conquest_schema(round: usize, value: serde_json::Value) -> Result<ConquestInfo, String> {
    if round < 229 {
        let s: ConquestSchemaPre229 = serde_json::from_value(value)
            .map_err(|e| format!("conquest schema (pre-229): {}", e))?;
        Ok(ConquestInfo {
            attacker: s.attacker,
            defender: s.defender,
            action_type: s.action_type,
            subjects: s.subjects,
            capitulation: s.capitulation,
            capitulation_event: s.capitulation_event,
            fallen_capital_remnant: s.fallen_capital_remnant,
            defender_admin_before: s.defender_admin_before,
            capital_index_after: None,
            defender_capital_territory_after: None,
            capital_index_before: None,
            original_capital_index: None,
            capital_moved: None,
        })
    } else if round == 229 {
        let s: ConquestSchema229 = serde_json::from_value(value)
            .map_err(|e| format!("conquest schema (229): {}", e))?;
        Ok(ConquestInfo {
            attacker: s.attacker,
            defender: s.defender,
            action_type: s.action_type,
            subjects: s.subjects,
            capitulation: s.capitulation,
            capitulation_event: s.capitulation_event,
            fallen_capital_remnant: s.fallen_capital_remnant,
            defender_admin_before: s.defender_admin_before,
            capital_index_after: Some(s.capital_index_after),
            defender_capital_territory_after: s.defender_capital_territory_after,
            capital_index_before: None,
            original_capital_index: None,
            capital_moved: None,
        })
    } else {
        // Round 230+
        let s: ConquestSchema230 = serde_json::from_value(value)
            .map_err(|e| format!("conquest schema (230+): {}", e))?;
        if s.defender_territory_rank_before.is_some() {
            return Err(format!(
                "defenderTerritoryRankBefore must always be null, got {:?}",
                s.defender_territory_rank_before
            ));
        }
        Ok(ConquestInfo {
            attacker: s.attacker,
            defender: s.defender,
            action_type: s.action_type,
            subjects: s.subjects,
            capitulation: s.capitulation,
            capitulation_event: s.capitulation_event,
            fallen_capital_remnant: s.fallen_capital,
            defender_admin_before: s.defender_admin_before,
            capital_index_after: Some(s.capital_index_after),
            defender_capital_territory_after: s.defender_capital_territory_after,
            capital_index_before: Some(s.capital_index_before),
            original_capital_index: Some(s.original_capital_index),
            capital_moved: Some(s.capital_moved),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Gamestate {
    epoch: usize,
    initial_month: u32,
    initial_year: i32,
    country_data: BTreeMap<u16, u16>,
    /// Capital overrides mapping country_id to current capital territory_id.
    capital_overrides: BTreeMap<u16, u16>,
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
        source: serde_json::Error,
    },
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Http(e) => write!(f, "HTTP error: {}", e),
            FetchError::Io(e) => write!(f, "IO error: {}", e),
            FetchError::Deserialization { endpoint, source } => {
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

fn try_fetch_round(round: usize, force_fetch: bool) -> Result<(SaveFile, PostFile, CapitalsFile), FetchError> {
    let round_dir = format!("data/{:06}", round);
    let save_path = format!("{}/save.json", round_dir);
    let post_path = format!("{}/post.json", round_dir);
    let capitals_path = format!("{}/capitals.json", round_dir);

    if !force_fetch && fs::metadata(&save_path).is_ok() && fs::metadata(&post_path).is_ok() && fs::metadata(&capitals_path).is_ok() {
        let save_body = fs::read_to_string(&save_path)?;
        let post_body = fs::read_to_string(&post_path)?;
        let capitals_body = fs::read_to_string(&capitals_path)?;

        let save: SaveFile =
            serde_json::from_str(&save_body).map_err(|e| FetchError::Deserialization {
                endpoint: "save",
                source: e,
            })?;

        let post: PostFile =
            serde_json::from_str(&post_body).map_err(|e| FetchError::Deserialization {
                endpoint: "post",
                source: e,
            })?;

        let capitals: CapitalsFile =
            serde_json::from_str(&capitals_body).map_err(|e| FetchError::Deserialization {
                endpoint: "capitals",
                source: e,
            })?;

        return Ok((save, post, capitals));
    }

    let save_url = format!("https://run5.worldwarbot.com/data/saves/{:06}.json", round);
    let mut save_res = ureq::get(&save_url).call()?;
    let save_body = save_res.body_mut().read_to_string()?;

    let post_url = format!("https://run5.worldwarbot.com/data/posts/{:06}.json", round);
    let mut post_res = ureq::get(&post_url).call()?;
    let post_body = post_res.body_mut().read_to_string()?;

    let capitals_url = format!("https://run5.worldwarbot.com/data/capitals/{:06}.json", round);
    let mut capitals_res = ureq::get(&capitals_url).call()?;
    let capitals_body = capitals_res.body_mut().read_to_string()?;

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

    let capitals_val_res: Result<serde_json::Value, _> = serde_json::from_str(&capitals_body);
    let (capitals_pretty, capitals_err) = match capitals_val_res {
        Ok(v) => (
            serde_json::to_string_pretty(&v).unwrap_or(capitals_body.clone()),
            None,
        ),
        Err(e) => (capitals_body.clone(), Some(e)),
    };

    fs::create_dir_all(&round_dir)?;
    fs::write(&save_path, &save_pretty)?;
    fs::write(&post_path, &post_pretty)?;
    fs::write(&capitals_path, &capitals_pretty)?;

    // Handle raw JSON syntax errors if any
    if let Some(e) = save_err {
        return Err(FetchError::Deserialization {
            endpoint: "save",
            source: e,
        });
    }
    if let Some(e) = post_err {
        return Err(FetchError::Deserialization {
            endpoint: "post",
            source: e,
        });
    }
    if let Some(e) = capitals_err {
        return Err(FetchError::Deserialization {
            endpoint: "capitals",
            source: e,
        });
    }

    // Now try schema validation against structs
    let save: SaveFile =
        serde_json::from_str(&save_pretty).map_err(|e| FetchError::Deserialization {
            endpoint: "save",
            source: e,
        })?;

    let post: PostFile =
        serde_json::from_str(&post_pretty).map_err(|e| FetchError::Deserialization {
            endpoint: "post",
            source: e,
        })?;

    let capitals: CapitalsFile =
        serde_json::from_str(&capitals_pretty).map_err(|e| FetchError::Deserialization {
            endpoint: "capitals",
            source: e,
        })?;

    Ok((save, post, capitals))
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
    let fetch_result = try_fetch_round(1, true);

    let mut generated_from_saves = false;
    let mut initial_month = chrono::Utc::now().month();
    let mut initial_year = chrono::Utc::now().year();

    if let Ok((_save, post, capitals)) = fetch_result {
        println!("Successfully fetched Round 1. Updating IDs.");
        generated_from_saves = true;

        let parts: Vec<&str> = post.caption.split(' ').collect();
        if parts.len() >= 2 {
            if let Some(m) = month_to_num(parts[0]) {
                let y: i32 = parts[1]
                    .trim_end_matches(',')
                    .parse()
                    .unwrap_or(initial_year);
                if m == 1 {
                    initial_month = 12;
                    initial_year = y - 1;
                } else {
                    initial_month = m - 1;
                    initial_year = y;
                }
            }
        }

        for (name, id) in capitals.original_capital_by_admin {
            id_map.insert(name, id.to_string());
        }

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
        capital_overrides: BTreeMap::new(),
    };

    fs::create_dir_all("data/000000")?;
    fs::write(
        "data/000000/gamestate.json",
        serde_json::to_string_pretty(&gamestate)?,
    )?;

    let targets_req = ureq::get("https://run5.worldwarbot.com/data/voronoi-neighbors.json").call();
    match targets_req {
        Ok(mut res) => {
            let targets_json: String = res.body_mut().read_to_string()?;
            fs::write("data/targets.json", targets_json)?;
            println!("Fetched and updated data/targets.json.");
        }
        Err(e) => {
            eprintln!(
                "Failed to fetch voronoi-neighbors.json: {}. Retaining existing file if present.",
                e
            );
        }
    }

    let countries_req = ureq::get("https://run5.worldwarbot.com/data/countries.json").call();
    match countries_req {
        Ok(mut res) => {
            let countries_json: String = res.body_mut().read_to_string()?;
            fs::write("data/countries.json", countries_json)?;
            println!("Fetched and updated data/countries.json.");
        }
        Err(e) => {
            eprintln!(
                "Failed to fetch countries.json: {}. Retaining existing file if present.",
                e
            );
        }
    }

    if generated_from_saves {
        println!(
            "Synced starting gamestate and cleared logs at epoch 0."
        );
    }

    Ok(())
}

fn get_latest_local_round() -> Option<usize> {
    let entries = fs::read_dir("data").ok()?;
    let mut max_round = None;

    for entry in entries.flatten() {
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(round) = name.parse::<usize>() {
                        if fs::metadata(format!("data/{:06}/gamestate.json", round)).is_ok() {
                            max_round = Some(max_round.map_or(round, |m| std::cmp::max(m, round)));
                        }
                    }
                }
            }
        }
    }
    max_round
}

#[derive(Deserialize, Debug)]
struct DataInfo {
    iteration: usize,
}

pub fn update_gamestate(force_fetch: bool) -> Result<usize, Box<dyn std::error::Error>> {
    let mut data_req = ureq::get("https://run5.worldwarbot.com/data/data.json").call()?;
    let data_info: DataInfo = data_req.body_mut().read_json()?;
    let max_iter = data_info.iteration;

    let local_round = get_latest_local_round().unwrap_or(0);
    let gamestate_path = format!("data/{:06}/gamestate.json", local_round);
    let gamestate_str = fs::read_to_string(gamestate_path)?;
    let mut current_state: Gamestate = serde_json::from_str(&gamestate_str)?;
    let targets_data = crate::utils::read_targets_data();
    let country_rows = crate::utils::read_country_data();
    let name_to_id: HashMap<String, u16> = country_rows
        .iter()
        .map(|(&id, c)| (c.name.clone(), id))
        .collect();

    let local_round = current_state.epoch;

    // Build the capital map from stored overrides (country_id -> current capital territory_id).
    // For rounds < 229, this will always be empty: every capital equals its country_id.
    // Represented as: absence in map means capital == country_id.
    // We maintain this across rounds and persist it back into gamestate.capital_overrides.
    // Note: current_state.capital_overrides has already been deserialized (default = empty).
    // We use a working HashMap for fast lookup during the loop.
    let mut capitals: HashMap<u16, u16> = current_state
        .capital_overrides
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();

    if local_round >= max_iter {
        println!("Gamestate already up to date at round {}.", max_iter);
        return Ok(0);
    }

    for round in (local_round + 1)..=max_iter {
        let (save, post, capitals_file) = match try_fetch_round(round, force_fetch) {
            Ok(data) => data,
            Err(e) => {
                match e {
                    FetchError::Deserialization {
                        endpoint, source, ..
                    } => {
                        eprintln!(
                            "Round {}: Deserialization failure for {}: {}",
                            round, endpoint, source
                        );
                    }

                    _ => {
                        eprintln!("Round {}: Failed to fetch data: {}", round, e);
                    }
                }
                std::process::exit(1);
            }
        };

        let mut any_unexpected = false;

        if capitals_file.iteration != round {
            eprintln!("Round {}: capitals.json iteration {} does not match", round, capitals_file.iteration);
            any_unexpected = true;
        }

        for (admin, cap_id) in &capitals_file.original_capital_by_admin {
            if name_to_id.get(admin) != Some(cap_id) {
                eprintln!("Round {}: capitals.json originalCapital {} {} != name_to_id", round, admin, cap_id);
                any_unexpected = true;
            }
        }

        // Parse the conquest sub-object with strict per-round schema validation.
        let conquest_info = match parse_conquest_schema(round, post.conquest) {
            Ok(info) => info,
            Err(e) => {
                eprintln!("Round {}: Conquest schema error: {}", round, e);
                std::process::exit(1);
            }
        };

        match &save.conquests.1 {
            ConquestData::Conquer(att_t_id, def_t_id, subjects) => {
                if post.action_type != "conquest" {
                    eprintln!(
                        "Round {}: Expected post type 'conquest' for Conquer shape, got '{}'",
                        round, post.action_type
                    );
                    any_unexpected = true;
                }
                if conquest_info.action_type != "conquer" {
                    eprintln!("Round {}: Expected conquest.action_type 'conquer' for Conquer shape, got '{}'", round, conquest_info.action_type);
                    any_unexpected = true;
                }
                if *att_t_id != conquest_info.attacker {
                    eprintln!(
                        "Round {}: Save attacker territory {} != post attacker territory {}",
                        round, att_t_id, conquest_info.attacker
                    );
                    any_unexpected = true;
                }
                if *def_t_id != conquest_info.defender {
                    eprintln!(
                        "Round {}: Save defender territory {} != post defender territory {}",
                        round, def_t_id, conquest_info.defender
                    );
                    any_unexpected = true;
                }
                if subjects.len() != 1 || subjects[0] != *def_t_id {
                    eprintln!(
                        "Round {}: Expected subjects [{}] in save Conquer shape, got {:?}",
                        round, def_t_id, subjects
                    );
                    any_unexpected = true;
                }
            }
            ConquestData::Riot(t_id1, t_id2) => {
                if post.action_type != "riot" {
                    eprintln!(
                        "Round {}: Expected post type 'riot' for Riot shape, got '{}'",
                        round, post.action_type
                    );
                    any_unexpected = true;
                }
                if conquest_info.action_type != "riot" {
                    eprintln!(
                        "Round {}: Expected conquest.action_type 'riot' for Riot shape, got '{}'",
                        round, conquest_info.action_type
                    );
                    any_unexpected = true;
                }
                if t_id1 != t_id2 {
                    eprintln!(
                        "Round {}: Riot shape expects identical IDs in save file, got {} and {}",
                        round, t_id1, t_id2
                    );
                    any_unexpected = true;
                }
                if *t_id1 != conquest_info.attacker || *t_id1 != conquest_info.defender {
                    eprintln!(
                        "Round {}: Save riot ID {} does not match post attacker/defender {}/{}",
                        round, t_id1, conquest_info.attacker, conquest_info.defender
                    );
                    any_unexpected = true;
                }
                if !conquest_info.subjects.is_empty() {
                    eprintln!(
                        "Round {}: Expected zero subjects in post for riot, got {}",
                        round,
                        conquest_info.subjects.len()
                    );
                    any_unexpected = true;
                }
            }
        }

        let territory_id = match &save.conquests.1 {
            ConquestData::Conquer(_, def_t_id, _) => *def_t_id,
            ConquestData::Riot(t_id, _) => *t_id,
        };
        let conquered_territory_id = territory_id as u16;
        let id_owners: HashMap<u16, u16> = current_state
            .country_data
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect();

        // Determine the defender's current capital before this round's conquest.
        // Before round 229 this is always the original (country_id).
        // From round 229 it may be overridden.
        let defender_current_capital_before: u16;

        let (attacker_country_id, defender_country_id) = if post.action_type == "conquest" {
            (name_to_id[&post.attacker], name_to_id[&post.defender])
        } else {
            // Riot case: post.territory is the name of the new independent country.
            // post.attacker is the country it rose against.
            if post.attacker != post.defender {
                eprintln!(
                    "Round {}: Expected attacker == defender for riot post, got {} and {}",
                    round, post.attacker, post.defender
                );
                any_unexpected = true;
            }
            (name_to_id[&post.territory], name_to_id[&post.attacker])
        };

        // Now we know the defender_country_id, resolve its current capital.
        defender_current_capital_before = *capitals
            .get(&defender_country_id)
            .unwrap_or(&defender_country_id);

        let attacking_territory_id = conquest_info.attacker as u16;

        // Validations before state change
        if id_owners[&conquered_territory_id] != defender_country_id {
            eprintln!("Round {}: Defender mismatch for territory {}. Expected owner (from API name: {}) {}, got {}", round, conquered_territory_id, post.defender, defender_country_id, id_owners[&conquered_territory_id]);
            any_unexpected = true;
        }

        // defenderAdminBefore must equal the defender's name from the post
        if conquest_info.defender_admin_before != post.defender {
            eprintln!(
                "Round {}: defenderAdminBefore '{}' != post.defender '{}'",
                round, conquest_info.defender_admin_before, post.defender
            );
            any_unexpected = true;
        }

        // Compute defender territory count before state change (needed for several validations)
        let defender_territories_before = current_state
            .country_data
            .values()
            .filter(|&owner| *owner == defender_country_id)
            .count();
        
        let lost_count = 1 + conquest_info.subjects.len();
        let completely_defeated = lost_count >= defender_territories_before;

        // Validate fallen_capital_remnant: must be true iff the conquered territory is the
        // defender's current capital AND the defender survives (has remaining territories)
        // AND it is NOT a capitulation (capitulations handle their own captions).
        let expected_fallen_capital_remnant =
            conquered_territory_id == defender_current_capital_before && 
            !completely_defeated && 
            !conquest_info.capitulation;
        if conquest_info.fallen_capital_remnant != expected_fallen_capital_remnant {
            eprintln!(
                "Round {}: fallen_capital_remnant mismatch. Expected {} (conquered={}, defender capital={}, eliminated={}, capitulation={}), got {}",
                round, expected_fallen_capital_remnant, conquered_territory_id,
                defender_current_capital_before, completely_defeated, conquest_info.capitulation,
                conquest_info.fallen_capital_remnant
            );
            any_unexpected = true;
        }

        if post.action_type == "conquest" {
            if conquest_info.capitulation {
                let event = conquest_info.capitulation_event.as_ref().unwrap();
                let ceded_ids: Vec<u16> = conquest_info
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
                    conquest_info.defender as u16,
                    &ceded_ids,
                    &id_owners,
                    &targets_data,
                ) {
                    eprintln!("Round {}: Capitulation validation failed: {}", round, e);
                    any_unexpected = true;
                }
            } else {
                if !conquest_info.subjects.is_empty() {
                    eprintln!(
                        "Round {}: Expected zero subjects in post for non-capitulating conquest, got {}",
                        round,
                        conquest_info.subjects.len()
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

        // Apply state change
        current_state
            .country_data
            .insert(conquered_territory_id, attacker_country_id);
        if post.action_type == "conquest" {
            for sub_val in &conquest_info.subjects {
                let sub_id = sub_val
                    .as_u64()
                    .map(|v| v as u16)
                    .unwrap_or_else(|| sub_val.as_str().and_then(|s| s.parse().ok()).unwrap_or(0));
                current_state
                    .country_data
                    .insert(sub_id, attacker_country_id);
            }
        }

        // --- End of State Change for Country Owners ---

        let id_owners_after: HashMap<u16, u16> = current_state
            .country_data
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect();

        // Update capital tracking and validate capitals_file.current_capital_by_admin
        for (admin, &current_cap_id) in &capitals_file.current_capital_by_admin {
            let country_id = match name_to_id.get(admin) {
                Some(&id) => id,
                None => {
                    eprintln!("Round {}: Unknown admin in capitals.json: {}", round, admin);
                    any_unexpected = true;
                    continue;
                }
            };

            let is_country_eliminated = !id_owners_after.values().any(|&o| o == country_id);
            let prev_cap_id = *capitals.get(&country_id).unwrap_or(&country_id);

            let expected_cap_id = if is_country_eliminated {
                current_cap_id // Accept any capital ID for eliminated countries (as per user request)
            } else if country_id == defender_country_id {
                if post.action_type == "riot" {
                    if conquered_territory_id == defender_current_capital_before {
                        current_cap_id // Can be anything, ownership handled below
                    } else {
                        defender_current_capital_before
                    }
                } else {
                    // Conquest case
                    if completely_defeated {
                        current_cap_id // API simply leaves the capital as is when defeated
                    } else if round >= 229 {
                        conquest_info.capital_index_after.unwrap_or(current_cap_id as usize) as u16
                    } else if conquered_territory_id == defender_current_capital_before {
                        current_cap_id // Can be anything, ownership handled below
                    } else {
                        defender_current_capital_before
                    }
                }
            } else if country_id == attacker_country_id && conquered_territory_id == attacker_country_id {
                // Attacker regained their original capital (via conquest or riot)
                attacker_country_id
            } else if id_owners.get(&prev_cap_id) != Some(&country_id) {
                // Relocation allowed if previous capital was lost in a previous round but only updated now
                current_cap_id
            } else {
                prev_cap_id // Unchanged
            };

            if current_cap_id != expected_cap_id {
                eprintln!(
                    "Round {}: capitals.json current capital for {} is {}, expected {}",
                    round, admin, current_cap_id, expected_cap_id
                );
                any_unexpected = true;
            }

            // Global ownership check for non-eliminated countries.
            // Exception: during a capitulation, the defender sometimes picks a territory that was
            // lost in the SAME round as its new capital. The API is inconsistent here, so we relax
            // the check for the defender in those rounds.
            if !is_country_eliminated && !(country_id == defender_country_id && conquest_info.capitulation) {
                if id_owners_after.get(&current_cap_id) != Some(&country_id) {
                    eprintln!("Round {}: capitals.json assigned unowned territory {} to country {}", round, current_cap_id, admin);
                    any_unexpected = true;
                }
            }
        }

        // Validate that defenders that lost their capital got assigned a valid owned territory ID
        // (Moved and broadened to previous global check)

        // Apply state change for capitals directly from capitals.json
        capitals.clear();
        for (admin, &cap_id) in &capitals_file.current_capital_by_admin {
            let country_id = name_to_id[admin];
            if cap_id != country_id {
                capitals.insert(country_id, cap_id);
            }
        }

        // --- Validate capitalIndexAfter / defenderCapitalTerritoryAfter (Round 229+) ---
        if round >= 229 && post.action_type == "conquest" {
            match conquest_info.capital_index_after {
                None => {
                    eprintln!("Round {}: Missing capitalIndexAfter in round 229+ conquest", round);
                    any_unexpected = true;
                }
                Some(cai) => {
                    let cai = cai as u16;
                    if completely_defeated {
                        if cai != defender_country_id {
                            eprintln!(
                                "Round {}: capitalIndexAfter for eliminated defender should be {} (original), got {}",
                                round, defender_country_id, cai
                            );
                            any_unexpected = true;
                        }
                        if conquest_info.defender_capital_territory_after.is_some() {
                            eprintln!(
                                "Round {}: defenderCapitalTerritoryAfter must be null for eliminated defender",
                                round
                            );
                            any_unexpected = true;
                        }
                    } else {
                        let expected_cap = *capitals
                            .get(&defender_country_id)
                            .unwrap_or(&defender_country_id);
                        if cai != expected_cap {
                            eprintln!(
                                "Round {}: capitalIndexAfter {} does not match expected capital {} for defender {}",
                                round, cai, expected_cap, defender_country_id
                            );
                            any_unexpected = true;
                        }

                        // Validate defenderCapitalTerritoryAfter
                        match &conquest_info.defender_capital_territory_after {
                            None => {
                                if conquest_info.fallen_capital_remnant {
                                    eprintln!(
                                        "Round {}: defenderCapitalTerritoryAfter is null but capital changed (fallen_capital_remnant is true)",
                                        round
                                    );
                                    any_unexpected = true;
                                }
                            }
                            Some(name) => {
                                if !conquest_info.fallen_capital_remnant {
                                    eprintln!(
                                        "Round {}: defenderCapitalTerritoryAfter is not null '{}' but capital did not change (fallen_capital_remnant is false)",
                                        round, name
                                    );
                                    any_unexpected = true;
                                }
                                if let Some(expected_name) = country_rows.get(&cai).map(|c| &c.name) {
                                    if name != expected_name {
                                        eprintln!(
                                            "Round {}: defenderCapitalTerritoryAfter '{}' != expected name '{}' for id {}",
                                            round, name, expected_name, cai
                                        );
                                        any_unexpected = true;
                                    }
                                } else {
                                    eprintln!(
                                        "Round {}: capitalIndexAfter {} has no name in country_rows",
                                        round, cai
                                    );
                                    any_unexpected = true;
                                }
                            }
                        }

                        // Validate that the new capital is actually owned by the country.
                        if conquest_info.fallen_capital_remnant {
                            if let Err(e) = crate::game_utils::validate_new_capital(
                                cai,
                                defender_country_id,
                                &id_owners_after,
                            ) {
                                eprintln!("Round {}: New capital validation failed: {}", round, e);
                                any_unexpected = true;
                            }
                        }
                    }
                }
            }

            // Round 230+: validate additional capital-tracking fields
            if round >= 230 {
                // capitalIndexBefore must equal the defender's current capital BEFORE the conquest
                if let Some(cib) = conquest_info.capital_index_before {
                    if cib as u16 != defender_current_capital_before {
                        eprintln!(
                            "Round {}: capitalIndexBefore {} != expected defender capital before {} for defender {}",
                            round, cib, defender_current_capital_before, defender_country_id
                        );
                        any_unexpected = true;
                    }
                }

                // originalCapitalIndex must always equal the defender's country ID
                if let Some(oci) = conquest_info.original_capital_index {
                    if oci as u16 != defender_country_id {
                        eprintln!(
                            "Round {}: originalCapitalIndex {} != defender country ID {}",
                            round, oci, defender_country_id
                        );
                        any_unexpected = true;
                    }
                }

                // capitalMoved must equal fallen_capital_remnant
                // (capital moves iff it was taken AND defender survived)
                if let Some(cm) = conquest_info.capital_moved {
                    if cm != conquest_info.fallen_capital_remnant {
                        eprintln!(
                            "Round {}: capitalMoved ({}) must equal fallen_capital_remnant ({})",
                            round, cm, conquest_info.fallen_capital_remnant
                        );
                        any_unexpected = true;
                    }
                }
            }
        }

        current_state.epoch = round;

        let remaining_count = current_state
            .country_data
            .values()
            .collect::<std::collections::HashSet<_>>()
            .len();
        let total_months = (current_state.initial_year as i64) * 12
            + (current_state.initial_month as i64 - 1)
            + (round as i64);

        let mut d_string = String::new();
        let is_reconquest =
            post.action_type == "conquest" && conquered_territory_id == attacker_country_id;
        // "previously occupied by" applies when the conquered territory is not its original owner's
        // (i.e., it was already stolen), UNLESS this is the attacker reconquering their own land.
        // For a fallen capital, the capital territory may have been the defender's own original
        // territory — but if fallen_capital_remnant=true it was the defender's *current* capital
        // (not necessarily the defender's original territory). The "previously occupied by" check
        // is about whether the *territory_id* was originally owned by the current defender, which
        // is still: id_owners[conquered] != conquered (territory was never the territory's original owner).
        if !is_reconquest && id_owners[&conquered_territory_id] != conquered_territory_id {
            d_string.push_str(&format!(" previously occupied by {}", post.defender));
        }

        if completely_defeated {
            d_string.push_str(&format!(
                ".\n{} has been completely defeated.\n{_e} countries remaining.",
                post.defender,
                _e = remaining_count
            ));
        } else if conquest_info.capitulation {
            let event = conquest_info.capitulation_event.as_ref().unwrap();
            let ceded = event.territories_ceded;
            d_string.push_str(&format!(
                ".\n{} capitulated, ceding {} additional territor{} to {}.",
                post.defender,
                ceded,
                if ceded == 1 { "y" } else { "ies" },
                post.attacker
            ));
        } else if conquest_info.fallen_capital_remnant {
            if round >= 229 {
                // New caption: capital relocates to a named territory
                let new_capital_name = conquest_info.defender_capital_territory_after
                    .as_deref()
                    .unwrap_or("");
                d_string.push_str(&format!(
                    ".\nThe government of {} relocated its capital to {} and continues in exile.",
                    post.defender, new_capital_name
                ));
            } else {
                d_string.push_str(&format!(
                    ".\nThe government of {} continues in exile, based in its remaining territories.",
                    post.defender
                ));
            }
        } else {
            d_string.push('.');
        }

        let date_prefix = format!(
            "{} {}, ",
            num_to_month(((total_months % 12) + 1) as u32),
            total_months / 12
        );

        let event_text = if post.action_type == "conquest" {
            if is_reconquest {
                format!(
                    "{} reconquered its homeland from {}{}",
                    post.attacker, post.defender, d_string
                )
            } else {
                format!(
                    "{} conquered {} territory{}",
                    post.attacker, post.territory, d_string
                )
            }
        } else {
            let riot_suffix = if round < 85 {
                "gained independence."
            } else {
                "reunited its homeland."
            };
            format!(
                "{} rose against {} and {}",
                post.territory, post.attacker, riot_suffix
            )
        };

        let expected_caption = if round < 167 {
            format!("{date_prefix}{event_text}\nCheck the full map at https://worldwarbot.com")
        } else {
            format!("{date_prefix}{event_text}")
        };

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
            eprintln!("Stopping simulation because validation mismatches were found.");
            std::process::exit(1);
        }

        // Persist capital overrides back into current_state before saving
        current_state.capital_overrides = capitals
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect();

        let mut lines: Vec<&str> = post.caption.lines().collect();
        if let Some(last_line) = lines.last() {
            if last_line.contains("worldwarbot.com") {
                lines.pop();
            }
        }
        let summary = format!("Round {}: {}", round, lines.join(" "));
        println!("{}", summary);

        fs::create_dir_all(format!("data/{:06}", round))?;
        fs::write(
            format!("data/{:06}/gamestate.json", round),
            serde_json::to_string_pretty(&current_state)?,
        )?;

        fs::create_dir_all("logs")?;
        let mut log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("logs/log.txt")?;
        use std::io::Write;
        writeln!(log_file, "{}", summary)?;
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

    Ok(n_processed)
}
