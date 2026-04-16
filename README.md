# wwsim — WorldWarBot Run Simulator

A Rust tool that simulates possible outcomes of the [@WorldWarBot](https://x.com/worldwarbot) game. Given the bot's current real-world game state, it runs the simulation N times in parallel and outputs statistics, allowing you to estimate which countries are most likely to win from the current position.

> Created by [@escalonn](https://github.com/escalonn), forked from the original by [@agubelu](https://github.com/agubelu).

---

## How WorldWarBot works

[@WorldWarBot](https://x.com/worldwarbot) is a Twitter/X bot that plays out a simulated world war between countries. Each turn revolves around a random territory acting as the "attacker." It might trigger a normal conquest, a riot, or force a capitulation:

- **Conquest**: A random viable neighboring territory (owned by a different country) is picked. If all immediate neighbors are self-owned, the search widens to a wider concentric shell of neighbors iteratively using a Breadth-First Search (BFS) until a foreign territory is found.
- **Capitulations**: If a specific country has its capital conquered and still owned at least 3 territories, there's a 1/3 probability that country immediately capitulates, randomly ceding up to half of its remaining territories to the attacker.
- **Riot**: If the randomly selected territory happens to be a conquered capital belonging to a previously eliminated country, there is a chance (`1 / (12 + epoch / 10)`) that a riot triggers. A riot grants independence, recreating a country, with a 20% cascading chance to cause adjacent subject states to simultaneously rebel and join the new country.

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

1. By default, the program automatically updates the local game state by fetching and parsing new rounds from the WorldWarBot server, bringing the local history in `data/` up to speed. Run with `--local` to skip this.
2. Load the target game state (territory ownership, epoch) from `data/<round>/gamestate.json`.
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
├── data/
│   ├── country_data.csv     # Master list of countries/territories
│   ├── targets.json         # Graph listing ~6 neighbors for each territory ID
│   ├── countries.json       # Color metadata for each country
│   └── 000XXX/              # Round-specific subfolders
│       ├── post.json        # API post data
│       ├── save.json        # API save data
│       └── gamestate.json   # Simulator state for that round
└── logs/
    ├── log.txt              # Historical timeline of all processed rounds
    ├── log_000XXX.md        # Detailed simulation report (requires --save)
    └── chart_000XXX.png     # Visual simulation results (requires --save)
```

---

## Output / Special Flags

| Flag | Purpose |
|---|---|
| `--reset` | Reset all game data and state based on the WorldWarBot server. Relies on round 1 data. |
| `--local` | Skip checking for new rounds from server; run from local data. |
| `--round <N>` | Start simulation from a specific historical round (e.g., `--round 100`). Defaults to latest. |
| `--force-fetch` | Force the scraper to re-download existing round data from the server even if present locally. |
| `--save` | Save the detailed Markdown report and PNG chart to the `logs/` directory. |
| `--if-updated` | Skip simulation if no new rounds were found during the update phase. |
| `--verbose` | Print the winner of each individual simulation run to stdout instead of summary statistics. |
| `--open` | Automatically open the generated chart PNG (requires `--save`). |


## Behavioral changes by round

The WorldWarBot game has evolved over time. The scraper validates captions and post-schema fields against the rules in effect for each round:

| Round | Change |
|---|---|
| **1–84** | Riot caption: `"… rose against … and gained independence."` |
| **85+** | Riot caption: `"… rose against … and reunited its homeland."` |
| **1–166** | All captions end with `"\nCheck the full map at https://worldwarbot.com"` |
| **167+** | That trailing URL line is omitted. |
| **1–228** | A country's capital is always its original territory (territory ID == country ID). Capitulations only trigger if the *original* capital is conquered by the attacker. |
| **229** | **Capital relocation**: when a country loses its current capital but still has territories, its capital moves to the nearest available territory via BFS from the original. If a country later reconquers its original capital, the capital reverts. Eliminated countries reset to their original capital (so future riots start from there). Capitulations can trigger on relocated capitals too. The `post.conquest` schema gains two new fields on every conquest: `capitalIndexAfter` (territory ID of the defender's capital after this round) and `defenderCapitalTerritoryAfter` (name of that territory, or `null` if the defender was eliminated or its capital did not move). The fallen-capital exile caption changes to: `"The government of X relocated its capital to Y and continues in exile."` |
| **230+** | **Schema evolution**: `fallenCapitalRemnant` is renamed to `fallenCapital`. New fields are introduced to `post.conquest` on every conquest: `capitalIndexBefore`, `originalCapitalIndex`, `capitalMoved` (true iff `fallenCapital` is true), and a placeholder `defenderTerritoryRankBefore` (always null). Strict validation ensures these new fields reflect the correct pre- and post-states. |

---

## Future work

- Experiment with adding "diff" displays to the chart, showing how much the displayed country's percentages went up or down since the last round, or probably, since one year before. Consider posting the chart on social media for every January.
- Improve the chart visuals for a mobile-Facebook-thumbnail viewing context.
- Build functionality (maybe in a separate script) to facilitate examining the round history to validate our model's assumptions: e.g. whether the capitulation chance is really 20%.
- In rounds 97 and 153, a country losing its capital gets assigned a capital that capitulated in the same round. The next round, it gets assigned a proper capital that it owns. This technically means we need capital-randomizing logic for runs that start on rounds like that for those specific countries. Except, the website's UI seems to know the correct capital even in that first round, but the logic is very obfuscated and minified. I'm not sure if it's randomized or deterministic.
