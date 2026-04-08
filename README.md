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

Each run simulates the game to completion from the current game state and prints the name of the winning country. All runs execute in parallel.

**Example — run 1000 simulations and tally results:**

```sh
cargo run --release 1000 | sort | uniq -c | sort -rn
```

---

## How the simulation works

Each simulated run proceeds as follows:

1. By default, the program automatically updates the local game state by fetching and parsing new rounds from the WorldWarBot API, bringing `data/gamestate.json` up to speed with the real world. Run with `--local` to skip this.
2. Load the current game state (territory ownership, epoch) from `data/gamestate.json`.
3. Load the geographic topology (the graph mapping each territory to 6 Voronoi-iteration neighbors) from `data/targets.json`.
4. Each turn:
   - Pick a randomly active territory to act as the primary node.
   - Test for Riot probability logic exclusively if it's the capital of a defeated sovereign state.
   - If a Riot doesn't occur, perform a standard BFS invasion. Seek out nearest geographical hostile elements. Roll a die post-conquest against Capitulation conditions if the defender's capital falls.
5. Repeat until exactly one sovereign state remains. Print its name natively.

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
    ├── country_data.csv     # Master list of countries/territories
    ├── targets.json         # Graph listing exactly 6 neighbors for each territory ID
    └── gamestate.json       # Current real-world game state (updated locally by the scraper)
```

---

## Data files

### `data/country_data.csv`

Semicolon-delimited. Columns: `id; name; longitude; latitude`

Maps each territory's numeric ID to its name and geographic coordinates. Run with `--reset` when a new game strictly begins so the scraper can dynamically associate and reset IDs based tightly on the server's unique initial mappings natively.

### `data/targets.json`

JSON Array of Arrays (`[[...],...]`)

A fully interconnected graph representing mapping structures across the globe. Each array corresponds to the territory ID index and contains precisely 6 neighboring elements forming a balanced geometric topological distribution avoiding natural location dominance. The automated scraper retrieves and manages this structure directly.

### `data/gamestate.json`

```json
{
  "epoch": 6,
  "initial_month": 3,
  "initial_year": 2026,
  "country_data": {
    "<territory_id>": "<owner_id>"
  }
}
```

Captures the real game's current state: how many turns have elapsed (`epoch`), the initial global month index, and who currently owns each territory. This is the starting point for all simulations.

This file is now automatically kept exactly in sync with the live bot via the built-in scraper logic executed upon process execution.

---

## Output / Special Flags

| Flag | Purpose |
|---|---|
| `--reset` | Reset all game data and state based on the WorldWarBot server. Relies on round 1 data. |
| `--local` | Skip checking for new rounds from WorldWarBot server. |

---

## Dependencies

| Crate | Purpose |
|---|---|
| [`rand`](https://crates.io/crates/rand) | Probability and algorithmic random shuffling of territories |
| [`rayon`](https://crates.io/crates/rayon) | Parallel simulation runs processing securely |
| [`serde`](https://crates.io/crates/serde) + [`serde_json`](https://crates.io/crates/serde_json) | JSON decoding standardly |
| [`counter`](https://crates.io/crates/counter) | Counting and caching algorithms logic efficiently |
| [`ureq`](https://crates.io/crates/ureq) | Polling endpoints synchronously efficiently |
| [`chrono`](https://crates.io/crates/chrono) | Syncing precise timeline references successfully |
