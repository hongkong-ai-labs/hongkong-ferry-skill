---
name: hongkong-ferry-skill
description: Hong Kong ferry routes, live next-sailing ETAs and published timetables for Hong Kong & Kowloon Ferry (HKKF: Central–Lamma, Peng Chau, Hei Ling Chau) and Sun Ferry (Central–Cheung Chau, Central–Mui Wo, North Point–Hung Hom, North Point–Kowloon City). Use for any question about Hong Kong ferries, outlying islands, piers, sailing times or the next boat.
license: MIT
---

# Hong Kong ferries

`hkferry` is a Rust CLI and MCP server for the public HKKF and Sun Ferry open
data feeds. It needs no API key and no account. It is an independent,
unofficial project.

Use it when the user asks about:

- the next ferry from Central to Cheung Chau, Mui Wo, Yung Shue Wan, Sok Kwu
  Wan, Peng Chau or Hei Ling Chau
- the next North Point ferry to Hung Hom or Kowloon City
- a ferry timetable for a weekday, Saturday, Sunday or public holiday
- which pier a route leaves from
- whether a live ETA exists or only a published timetable

## Before first use

Run `scripts/ensure-binary.sh` from the skill directory:

```console
$ bash scripts/ensure-binary.sh
/Users/you/.local/bin/hkferry
```

It prints the absolute path of a working binary. Use it for every command:

```bash
BIN="$(bash scripts/ensure-binary.sh)"
```

It checks, in order: `$HKFERRY_BIN`, `hkferry` on `PATH`,
`scripts/install.sh`, then `cargo build --release --locked` in the repo root.

### Install the binary directly

`npx skills add` installs the skill files. If the binary is missing, install it
with the release installer or cargo:

```bash
curl -fsSL https://raw.githubusercontent.com/hongkong-ai-labs/hongkong-ferry-skill/main/scripts/install.sh | sh
# or
cargo install --git https://github.com/hongkong-ai-labs/hongkong-ferry-skill --locked
```

Register it with any MCP client:

```bash
npx add-mcp "$(command -v hkferry) mcp" --name hongkong-ferry -a claude-code -a codex -a cursor -y
claude mcp add hongkong-ferry -- hkferry mcp
codex mcp add hongkong-ferry -- hkferry mcp
```

`add-mcp` detects the client configuration files in the current project; add
`-g` for a global install. `$(command -v hkferry)` keeps the entry correct
when the binary is on `PATH`; substitute an absolute path if your client does
not inherit the shell `PATH`.

## CLI

Prefer `--json` when you need to parse. Times are operator-local clock times
in Hong Kong (UTC+8).

```bash
$BIN routes                          # both operators
$BIN routes --operator hkkf
$BIN routes --operator sunferry --json

$BIN eta hkkf 2                      # Central -> Yung Shue Wan
$BIN eta hkkf 2 --direction inbound  # Yung Shue Wan -> Central
$BIN eta hkkf "sok kwu" --json
$BIN eta sunferry CECC               # Central -> Cheung Chau
$BIN eta sunferry "mui wo" --json

$BIN timetable hkkf 1 --direction outbound
$BIN timetable sunferry CECC --json
$BIN update --check
```

Routes can be selected by operator id (`2`), Sun Ferry code (`CECC`), timetable
slug (`central_cheungchau`) or a name substring (`cheung chau`). If a name
matches more than one route, the first match wins; use the id for certainty.

### Covered operators

| Operator | Routes | Live ETA | Timetable |
| --- | --- | --- | --- |
| HKKF | 1 Central–Sok Kwu Wan, 2 Central–Yung Shue Wan, 3 Central–Peng Chau, 4 Peng Chau–Hei Ling Chau | yes | yes |
| Sun Ferry | CECC Central–Cheung Chau, CEMW Central–Mui Wo, NPHH North Point–Hung Hom, NPKC North Point–Kowloon City | yes | yes |

`--direction outbound` means away from the route's origin pier and `inbound`
means towards it, as HKKF publishes them. Sun Ferry's ETA feed does not publish
a direction, so both directions appear together.

HKKF's published timetable encodes markers on some sailings: `*` (for example
only on Saturdays), `#` (freight or advance-booking condition) and `^` (last
sailing). Sun Ferry timetables use numeric remark codes; `1` and similar refer
to the footnotes in the operator's published timetable.

## Reading results

`eta` returns:

| Field | Meaning |
| --- | --- |
| `depart` | Scheduled departure, `HH:MM` |
| `eta` | Estimated arrival time as published, `HH:MM` |
| `eta_at` | Full ETA timestamp when HKKF publishes one |
| `vessel` | Vessel code when Sun Ferry publishes one |
| `remark` | Operator remark, when present |

An empty `sailings` array means the operator currently publishes no upcoming
sailing — commonly after the last boat. Say that, and give the first published
sailing from `timetable` rather than inventing one.

Sun Ferry's live feed can include a sailing with a departure time but no `eta`
yet. That is a scheduled sailing, not a confirmed arrival.

## MCP

```bash
$BIN mcp
```

```json
{
  "mcpServers": {
    "hongkong-ferry": {
      "command": "/absolute/path/to/hkferry",
      "args": ["mcp"]
    }
  }
}
```

Tools:

| Tool | Arguments |
| --- | --- |
| `list_ferry_routes` | `{ "operator"?: "all"\|"hkkf"\|"sunferry" }` |
| `get_ferry_eta` | `{ "operator": "hkkf"\|"sunferry", "route": string, "direction"?: "outbound"\|"inbound" }` |
| `get_ferry_timetable` | `{ "operator": "hkkf"\|"sunferry", "route": string, "direction"?: "outbound"\|"inbound" }` |

## Cautions

- Live ETAs are predictions. Typhoon, rainstorm, fog and special traffic
  arrangements can suspend or change sailings without a live update.
- Not every Hong Kong ferry is here. Star Ferry, Fortune Ferry, Coral Ferry and
  the Discovery Bay / Park Island ferries are separate services.
- The HKKF timetable feed sometimes embeds HTML in its remark columns. This
  tool extracts the sailing times and markers, not the full styled notice.
- Do not present this as an official operator channel. For a disruption, check
  the operator's website or pier notice.

## Sources

- HKKF: `https://www.hkkfeta.com/opendata/route/`, `/eta/{route}/{direction}`,
  `/time_table/{route}/{direction}`
- Sun Ferry: `https://www.sunferry.com.hk/eta/?route={code}` and
  `/eta/timetable/SunFerry_{slug}_timetable_eng.csv`

Both are published under DATA.GOV.HK. Responses are cached briefly in-process
(30 seconds for ETA, 6 hours for routes and timetables).
