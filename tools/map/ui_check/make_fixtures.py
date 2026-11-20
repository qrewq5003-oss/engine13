"""Recorded inputs of the province map's stand (docs/engine2_map.md §37.7).

Writes dev/fixtures/<scenario>_start.json — owners and capitals of the start, from
provinces.toml and scenario.toml — and <scenario>_after.json: the same with one province
moved to another owner and three units, two of them in one province. The stand feeds these
to `ProvinceMap` in place of the engine's view; nothing here is a fact of a played game.
The names of the fields are those of the engine's `WorldView` (plan §31.77): a province's `id`
and `owner`, a polity's `id`, `name` and `capital`, a unit's `id`, `owner` and `position`.

    python tools/map/ui_check/make_fixtures.py
"""
import json
import pathlib
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[3]
SCENARIO = "milan_1477"
# The move of the second recording: a province of Venice to Milan.
MOVED, NEW_OWNER = "bergamo", "milan"
UNITS = [
    {"id": "unit-1", "owner": "milan", "position": "milano"},
    {"id": "unit-2", "owner": "milan", "position": "milano"},
    {"id": "unit-3", "owner": "venice", "position": "verona"},
]


def main():
    data = REPO / "public" / "geodata2" / SCENARIO
    provinces = tomllib.loads((data / "provinces.toml").read_text(encoding="utf-8"))["provinces"]
    scenario = tomllib.loads((data / "scenario.toml").read_text(encoding="utf-8"))
    start = {
        "viewer": "milan",
        "provinces": [{"id": pid, "owner": p["owner"]} for pid, p in provinces.items()],
        "actors": [{"id": aid, "name": a["name"], "capital": a["capital"]} for aid, a in scenario["actors"].items()],
        "units": [],
    }
    assert provinces[MOVED]["owner"] != NEW_OWNER
    after = dict(start)
    after["provinces"] = [{"id": p["id"], "owner": NEW_OWNER if p["id"] == MOVED else p["owner"]} for p in start["provinces"]]
    after["units"] = UNITS
    out = REPO / "dev" / "fixtures"
    out.mkdir(parents=True, exist_ok=True)
    for name, view in (("start", start), ("after", after)):
        path = out / f"{SCENARIO}_{name}.json"
        path.write_text(json.dumps(view, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")
        print(f"wrote {path.relative_to(REPO)}: {len(view['provinces'])} provinces, {len(view['actors'])} actors, {len(view['units'])} units")


if __name__ == "__main__":
    main()
