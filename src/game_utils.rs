use std::collections::{HashMap, HashSet};
use rand::seq::SliceRandom;
use rand::random;

// `find_attack_targets(attacker_id, owners_data, targets_data) -> Vec<u16>`
// Implement the BFS to find the closest foreign territory shell and return all targets in it.
pub fn find_attack_targets(
    attacker_id: u16,
    owners_data: &HashMap<u16, u16>,
    targets_data: &HashMap<u16, Vec<u16>>
) -> Vec<u16> {
    let attacker_owner = owners_data[&attacker_id];
    
    let mut visited = HashSet::new();
    visited.insert(attacker_id);
    
    let mut current_shell = targets_data[&attacker_id].clone();
    current_shell.retain(|&id| visited.insert(id));
    
    loop {
        if current_shell.is_empty() {
            // Should not happen unless the whole world is owned by the attacker.
            return Vec::new();
        }

        let foreign_targets: Vec<u16> = current_shell.iter()
            .copied()
            .filter(|&id| owners_data[&id] != attacker_owner)
            .collect();
            
        if !foreign_targets.is_empty() {
            return foreign_targets;
        }
        
        // Build next shell
        let mut next_shell = Vec::new();
        for id in current_shell {
            for neighbor in &targets_data[&id] {
                if visited.insert(*neighbor) {
                    next_shell.push(*neighbor);
                }
            }
        }
        current_shell = next_shell;
    }
}

// `perform_riot(riot_territory_id, owners_data, owns_data, targets_data)`
pub fn perform_riot(
    riot_territory_id: u16,
    owners_data: &mut HashMap<u16, u16>,
    owns_data: &mut HashMap<u16, u16>,
    targets_data: &HashMap<u16, Vec<u16>>,
    remaining: &mut HashSet<u16>
) {
    let old_owner_id = owners_data[&riot_territory_id];

    // Riot expands via BFS from riot_territory_id
    // It captures eligible neighbors recursively with fixed chance.
    let mut rioting_territories = vec![riot_territory_id];
    
    let mut visited = HashSet::new();
    visited.insert(riot_territory_id);

    // Initial frontier
    let mut eligible_neighbors: Vec<u16> = targets_data[&riot_territory_id]
        .iter()
        .copied()
        .filter(|&neighbor| owners_data[&neighbor] == old_owner_id && neighbor != old_owner_id)
        .collect();

    // Spread the riot!
    let spread_chance = 0.2; // Based on the proposal

    let mut rng = rand::thread_rng();

    // We add territories one at a time. The loop stops as soon as the chance roll fails once.
    loop {
        // Collect all currently eligible neighbors adjacent to *any* rioting territory
        let current_eligible: Vec<u16> = eligible_neighbors.iter()
            .copied()
            .filter(|&id| !visited.contains(&id))
            .collect();
            
        if current_eligible.is_empty() {
            break;
        }

        let chosen_neighbor = current_eligible.choose(&mut rng).copied().unwrap();
        visited.insert(chosen_neighbor);
        
        let roll: f64 = random();
        if roll < spread_chance {
            // Success! Add it to the riot.
            rioting_territories.push(chosen_neighbor);
            
            // Add its eligible neighbors to the pool (that aren't visited)
            for &n_id in &targets_data[&chosen_neighbor] {
                if !visited.contains(&n_id) && owners_data[&n_id] == old_owner_id && n_id != old_owner_id {
                    // Only add if it's not already in eligible_neighbors
                    if !eligible_neighbors.contains(&n_id) {
                        eligible_neighbors.push(n_id);
                    }
                }
            }
        } else {
            // Failure! Riot stops here.
            break;
        }
    }
    
    // Transfer ownership
    for &id in &rioting_territories {
        owners_data.insert(id, riot_territory_id);
        *owns_data.entry(riot_territory_id).or_insert(0) += 1;
        *owns_data.entry(old_owner_id).or_insert(0) -= 1;
    }
    
    // The newly independent territory re-enters the game as its own country.
    remaining.insert(riot_territory_id);
    
    if owns_data[&old_owner_id] == 0 {
        remaining.remove(&old_owner_id);
    }
}

pub fn perform_conquest(
    attacker_id: u16, // actually the territory selected to attack
    target_territory_id: u16,
    owners_data: &mut HashMap<u16, u16>,
    owns_data: &mut HashMap<u16, u16>,
    targets_data: &HashMap<u16, Vec<u16>>,
    remaining: &mut HashSet<u16>
) {
    let attacker_country_id = owners_data[&attacker_id];
    let original_conquered_country_id = owners_data[&target_territory_id];
    
    // Check for capitulation event
    let mut ceded_territories = HashSet::new();
    ceded_territories.insert(target_territory_id);
    
    let defender_num_territories = owns_data[&original_conquered_country_id];
    let is_capital_loss = target_territory_id == original_conquered_country_id;
    
    if is_capital_loss && defender_num_territories >= 3 {
        // Capitulation check
        let capitulation_chance = 1.0 / 3.0; // 1/3
        if random::<f64>() < capitulation_chance {
            // Capitulates!
            let max_additional = (defender_num_territories as f64 / 2.0).ceil() as u32;
            let n_additional = (rand::random::<u32>() % max_additional) + 1; // 1 to ceil(x/2)
            
            // Start BFS from target_territory_id to find nearest N territories owned by the same country.
            // Find at same "depth", pick from them randomly.
            // Breadth First Search using shells
            let mut visited = HashSet::new();
            visited.insert(target_territory_id);
            
            let mut current_shell = targets_data[&target_territory_id].clone();
            current_shell.retain(|&id| visited.insert(id));
            
            let mut additional_ceded = 0;
            
            while additional_ceded < n_additional {
                if current_shell.is_empty() {
                    break;
                }
                
                let eligible_in_shell: Vec<u16> = current_shell.iter()
                    .copied()
                    .filter(|&id| owners_data[&id] == original_conquered_country_id)
                    .collect();
                    
                if eligible_in_shell.is_empty() {
                    let mut next_shell = Vec::new();
                    for id in current_shell {
                        for neighbor in &targets_data[&id] {
                            if visited.insert(*neighbor) {
                                next_shell.push(*neighbor);
                            }
                        }
                    }
                    current_shell = next_shell;
                    continue;
                }
                
                // Pick from them randomly until shell exhausted or quota met
                let mut shuffled = eligible_in_shell.clone();
                shuffled.shuffle(&mut rand::thread_rng());
                
                for id in shuffled {
                    if additional_ceded >= n_additional {
                        break;
                    }
                    ceded_territories.insert(id);
                    additional_ceded += 1;
                }
                
                // Advance shell
                let mut next_shell = Vec::new();
                for id in current_shell {
                    for neighbor in &targets_data[&id] {
                        if visited.insert(*neighbor) {
                            next_shell.push(*neighbor);
                        }
                    }
                }
                current_shell = next_shell;
            }
        }
    }

    let n_ceded = ceded_territories.len() as u16;
    
    *owns_data.entry(attacker_country_id).or_insert(0) += n_ceded;
    *owns_data.entry(original_conquered_country_id).or_insert(0) -= n_ceded;
    
    for id in ceded_territories {
        owners_data.insert(id, attacker_country_id);
    }
    
    if owns_data[&original_conquered_country_id] == 0 {
        remaining.remove(&original_conquered_country_id);
    }
}

pub fn validate_capitulation(
    capital_id: u16,
    ceded_additional_ids: &[u16],
    owners_before: &HashMap<u16, u16>,
    targets_data: &HashMap<u16, Vec<u16>>,
) -> Result<(), String> {
    let defender_id = owners_before[&capital_id];
    let n_before = owners_before
        .values()
        .filter(|&&o| o == defender_id)
        .count();
    let n_ceded_additional = ceded_additional_ids.len();

    // Rule: n_additional <= ceil(N/2) - 1
    let limit = ((n_before as f64) / 2.0).ceil() as usize - 1;
    if n_ceded_additional > limit {
        return Err(format!(
            "Capitulation ceded too many territories: {} > {}",
            n_ceded_additional, limit
        ));
    }

    // Rule: closest remaining territory must be no closer than any ceded territory.
    let mut distances = HashMap::new();
    let mut visited = HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    distances.insert(capital_id, 0);
    visited.insert(capital_id);
    queue.push_back(capital_id);

    while let Some(curr) = queue.pop_front() {
        let d = distances[&curr];
        for &neighbor in &targets_data[&curr] {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                distances.insert(neighbor, d + 1);
                queue.push_back(neighbor);
            }
        }
    }

    let ceded_set: HashSet<u16> = ceded_additional_ids.iter().copied().collect();
    let mut max_ceded_dist = 0;
    for &id in &ceded_set {
        let d = *distances
            .get(&id)
            .ok_or_else(|| format!("Ceded territory {} unreachable from capital?", id))?;
        if d > max_ceded_dist {
            max_ceded_dist = d;
        }
    }

    // Check remaining territories
    for (&id, &owner) in owners_before {
        if owner == defender_id && id != capital_id && !ceded_set.contains(&id) {
            let d = *distances
                .get(&id)
                .ok_or_else(|| format!("Remaining territory {} unreachable from capital?", id))?;
            if d < max_ceded_dist {
                return Err(format!(
                    "Remaining territory {} is closer ({}) than some ceded territory (max dist {})",
                    id, d, max_ceded_dist
                ));
            }
        }
    }

    Ok(())
}
