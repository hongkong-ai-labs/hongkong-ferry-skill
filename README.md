# hkferry

Hong Kong ferry routes, live ETAs and timetables as a Rust CLI and MCP server.

Covers Hong Kong & Kowloon Ferry (HKKF) and Sun Ferry via their public,
keyless open data.

## Install

```bash
npx skills add hongkong-ai-labs/hongkong-ferry-skill
cargo install --path . --locked
sh scripts/install.sh
```

## CLI

```bash
hkferry routes
hkferry routes --operator hkkf
hkferry eta hkkf 2
hkferry eta hkkf 2 --direction inbound
hkferry eta sunferry CECC --json
hkferry timetable hkkf 1 --direction outbound
hkferry timetable sunferry CECC
hkferry mcp
```

## Routes

| Operator | Routes |
| --- | --- |
| HKKF | Central–Sok Kwu Wan, Central–Yung Shue Wan, Central–Peng Chau, Peng Chau–Hei Ling Chau |
| Sun Ferry | Central–Cheung Chau, Central–Mui Wo, North Point–Hung Hom, North Point–Kowloon City |

## MCP

```json
{
  "mcpServers": {
    "hongkong-ferry": { "command": "hkferry", "args": ["mcp"] }
  }
}
```

Tools: `list_ferry_routes`, `get_ferry_eta`, `get_ferry_timetable`.

## Limits

Live ETA is a prediction and can lag special arrangements. Star Ferry, Fortune
Ferry and the Discovery Bay / Park Island ferries are not included. Timetable
remarks are operator text; read the operator notice for a disruption.

## Licence

MIT. Ferry data remains subject to the operators' terms and DATA.GOV.HK terms.
