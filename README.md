# wwsim — WorldWarBot Run Simulator

A Rust tool that simulates possible outcomes of the [@WorldWarBot](https://twitter.com/worldwarbot) game. Given the bot's current real-world game state, it runs the simulation N times in parallel and prints the winning country for each run, allowing you to estimate which countries are most likely to win from the current position.

> Created by [@agubelu](https://twitter.com/agubelu), with contributions by [@escalonn](https://twitter.com/escalonn).

---

## How WorldWarBot works

[@WorldWarBot](https://twitter.com/worldwarbot) is a Twitter/X bot that plays out a simulated world war between countries. Each turn revolves around a random territory acting as the "attacker." It might trigger a normal conquest, a riot, or force a capitulation:

- **Conquest**: A random viable neighboring territory (owned by a different country) is picked. If all immediate neighbors are self-owned, the search widens to a wider concentric shell of neighbors iteratively using a Breadth-First Search (BFS) until a foreign territory is found.
- **Capitulations**: If a specific country has its initial capital conquered and still owned at least 3 territories, there's a strict 1/3 probability that country immediately capitulates, randomly ceding up to half of its remaining territories seamlessly to the attacker.
- **Riot**: If the randomly selected territory happens to be a conquered capital belonging to a previously eliminated country, there is a chance (`1 / (12 + epoch / 10)`) that a Riot triggers. A Riot grants independence, creating a sovereign nation, with a 20% cascading chance to cause adjacent subject states to simultaneously rebel and join the new nation.

The game ends when only one country controls all territories.

---

## Building

Requires [Rust](https://www.rust-lang.org/tools/install) (edition 2018).

```sh
cargo build --release
```

The binary must be run from the **project root**, as data files are loaded via hardcoded relative paths (`data/`).

---

## Usage

```sh
cargo run --release <number_of_runs>
```

Each run simulates the game to completion from the current game state and outputs statistics from the runs. All runs execute in parallel.

---

## How the simulation works

Each simulated run proceeds as follows:

1. By default, the program automatically updates the local game state by fetching and parsing new rounds from the WorldWarBot server, bringing `data/gamestate.json` up to speed. Run with `--local` to skip this.
2. Load the current game state (territory ownership, epoch) from `data/gamestate.json`.
3. Load the geographic topology (the graph mapping each territory to 6 Voronoi-iteration neighbors) from `data/targets.json`.
4. Each turn:
   - Pick a random territory as the attacker.
   - Chance to riot if it's the capital of a defeated country.
   - If a riot doesn't occur, search for a nearby foreign territory to attack and transfer ownership to the attacker. If it's the defender's capital, chance for "capitulation", which cedes additional territories.
5. Repeat until one country remains.

---

## Project structure

```
wwsim/
├── src/
│   ├── main.rs              # Entry point; runs scraper then the simulation loop
│   ├── scraper.rs           # Automates mapping IDs, fetching topology, and syncing states
│   ├── game_utils.rs        # Simulation mechanics: BFS logic, Riots, Capitulations, Conquest
│   ├── gamestate_reader.rs  # Parses gamestate.json into runtime data structures
│   └── utils.rs             # Reads logic data structures (targets.json, country_data.csv)
└── data/
    ├── country_data.csv     # Master list of countries/territories: Columns: `id;name;longitude;latitude`, scraped from server
    ├── targets.json         # Graph listing ~6 neighbors for each territory ID, scraped from server
    └── gamestate.json       # Current real-world game state (updated locally by the scraper)
```

---

## Output / Special Flags

| Flag | Purpose |
|---|---|
| `--reset` | Reset all game data and state based on the WorldWarBot server. Relies on round 1 data. |
| `--local` | Skip checking for new rounds from WorldWarBot server; run simulations from `data/gamestate.json`. |
| `--if-updated` | Skip simulation if no new rounds were found during the update phase. |
| `--verbose` | Print the winner of each individual simulation run to stdout instead of summary statistics. |
