"""Lay out a scenario's provinces on the common base (plan docs/engine2_map.md §3, §6, §7):
the layout table (layouts/<scenario>.toml) assigns blocks to provinces; the rules of §4 give
area, neighbours, coasts and sea neighbours; the land class is calibrated on the provinces by
the ceilings of their lands (§11a–§11c); the start population follows §11d.

    python -I -X utf8 tools/map/layout.py milan_1477 [--cache D:/map-cache]

Writes public/geodata2/<scenario>/provinces.geojson and provinces.toml; the measure and the
previews go to <cache>/out/<scenario>/.
"""

import argparse
import collections
import csv
import json
import pathlib
import sys
import tomllib

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import geopandas as gpd  # noqa: E402
import matplotlib  # noqa: E402

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
import shapely  # noqa: E402
from shapely.geometry import mapping  # noqa: E402

import base  # noqa: E402
import density  # noqa: E402
import rules  # noqa: E402
from density import A, D, H, I, S, TYPES  # noqa: E402

REPO = HERE.parent.parent
SIMPLIFY_DEG = 0.005  # the interface polygons (E5): ~0.5 km, below a pixel of the map at Italy's scale
STUB_FILL = 0.5       # population of a province of no land with a start population — a stub until E4


def read_base(base_dir: pathlib.Path):
    blocks = density.read_blocks(base_dir)
    with (base_dir / "blocks.csv").open(encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    bdata = {r["id"]: rules.Block(float(r["area_km2"]), float(r["raster_km2"]), float(r["rugged_km2"]),
                                  float(r["climate_km2"]), float(r["bw_km2"]), float(r["bs_km2"]), float(r["river_km"]))
             for r in rows}
    with (base_dir / "borders.csv").open(encoding="utf-8") as f:
        edges = [(r["a"], r["b"], float(r["km"])) for r in csv.DictReader(f)]
    with (base_dir / "coasts.csv").open(encoding="utf-8") as f:
        coasts = [(r["block"], r["zone"], float(r["km"])) for r in csv.DictReader(f)]
    shore = {b for b, _, km in coasts if km > 0}
    blocks["coastal"] = np.array([b in shore for b in blocks["id"]])
    return blocks, bdata, edges, coasts


def assign(layout, blocks):
    """Block index → province id: provinces in order, each takes the blocks it selects that no
    earlier province took."""
    owner_of = np.full(len(blocks["id"]), "", dtype=object)
    for prov in layout["province"]:
        m = density.region_mask(blocks, prov["parts"]) & (owner_of == "")
        if not m.any():
            raise SystemExit(f"province {prov['id']} selects no block")
        owner_of[m] = prov["id"]
    return owner_of


def province_lands(pids, owner_of, blocks, masks, lands_cfg, scenario_lands):
    """Each province's land: the innermost land of the scenario that holds the majority of its
    area, or None."""
    out = {}
    for pid in pids:
        m = owner_of == pid
        total = blocks["area"][m].sum()
        best = None
        for land in scenario_lands:
            region = lands_cfg[land]["region"] if land in lands_cfg and "region" in lands_cfg[land] else None
            share = blocks["area"][m & masks[region]].sum() / total
            if share >= 0.5 and (best is None or density.land_depth(land, lands_cfg) > density.land_depth(best, lands_cfg)):
                best = land
        out[pid] = best
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("scenario")
    ap.add_argument("--cache", default="D:/map-cache")
    args = ap.parse_args()
    cache = pathlib.Path(args.cache)
    base_dir = cache / "out" / "base"
    out = cache / "out" / args.scenario
    out.mkdir(parents=True, exist_ok=True)
    public = REPO / "public" / "geodata2" / args.scenario
    public.mkdir(parents=True, exist_ok=True)
    layout = tomllib.loads((HERE / "layouts" / f"{args.scenario}.toml").read_text(encoding="utf-8"))
    dcfg = tomllib.loads((HERE / "density.toml").read_text(encoding="utf-8"))
    p = rules.Params.from_toml(tomllib.loads((HERE / "params.toml").read_text(encoding="utf-8")))
    engine = tomllib.loads((REPO / "engine2" / "params.toml").read_text(encoding="utf-8"))
    balance = density.plague_balance_fill(engine)
    sc = dcfg["scenarios"][args.scenario]
    lands_cfg = dcfg.get("lands", {})
    rep = base.Report()

    blocks, bdata, edges, coasts = read_base(base_dir)
    owner_of = assign(layout, blocks)
    provs_cfg = {prov["id"]: prov for prov in layout["province"]}
    pids = sorted(provs_cfg)
    asg = {blocks["id"][k]: owner_of[k] for k in np.flatnonzero(owner_of != "")}
    facts = rules.provinces(asg, bdata, edges, coasts, p)
    rep.step(f"{len(pids)} provinces of {len(asg)} blocks")

    # Lands: a land's region is its own (rule 1) or that of its envelope observation.
    masks = density.region_masks(dcfg, blocks)
    obs, top = density.envelope(dcfg, blocks, masks)
    for land in sc["lands"]:
        if not (land in lands_cfg and "region" in lands_cfg[land]):
            lands_cfg.setdefault(land, {})["region"] = top[land]["region"]
    plands = province_lands(pids, owner_of, blocks, masks, lands_cfg, sc["lands"])
    order = sorted(sc["lands"], key=lambda land: (-density.land_depth(land, lands_cfg), sc["lands"].index(land)))

    def inside(land_of, land):
        while land_of is not None:
            if land_of == land:
                return True
            land_of = lands_cfg.get(land_of, {}).get("within")
        return False

    member = {land: np.array([inside(plands[pid], land) for pid in pids]) for land in order}
    ceiling = {land: top[land]["y"] if land in top and top[land].get("opened", True) else None for land in order}
    area = np.array([facts[pid].area_km2 for pid in pids], dtype=float)
    rugged = np.array([facts[pid].shares["rugged"] for pid in pids])
    bw = np.array([facts[pid].shares["bw"] for pid in pids])
    dry = bw + np.array([facts[pid].shares["bs"] for pid in pids])
    no_land = np.array([plands[pid] is None for pid in pids])

    variants = {}
    for arable in dcfg["calibration"]["arable"]:
        dens = np.zeros(5)
        for t in ("Arable", "Hills", "Steppe", "Desert"):
            dens[TYPES.index(t)] = density.E0[t][0]
        dens[A] = arable
        cls = density.calibrate_lands(order, member, ceiling, area, rugged, dry, bw, dens, p)
        cls[no_land] = density.rule_class(rugged[no_land], bw[no_land], dry[no_land], p)
        variants[arable] = (dens, cls)

    # Start population (§11d): one fill per land; a land's number covers its whole region, so the
    # share of it this scenario's provinces of the land carry is the share of the region's
    # capacity in them (a block of the region may sit in a province of another land).
    start = sc.get("start_population", {})
    k_of = {pid: k for k, pid in enumerate(pids)}
    block_prov = np.array([k_of.get(x, -1) for x in owner_of])

    def populate(dens, cls):
        cap = area * dens[cls]
        pop = np.full(len(pids), np.nan)
        for land in order:
            if land not in start:
                continue
            region = masks[lands_cfg[land]["region"]]
            inreg = region & (block_prov >= 0)
            bcap = blocks["area"][inreg] * dens[cls[block_prov[inreg]]]
            ours = np.array([member[land][k] for k in block_prov[inreg]])
            share = bcap[ours].sum() / bcap.sum()
            people = start[land] * 1e6 * share
            m = member[land]
            setv = m & ~np.isnan(pop)
            free = m & np.isnan(pop)
            if free.any():  # an outer land wholly split into inner lands has no province left
                pop[free] = (people - pop[setv].sum()) / cap[free].sum() * cap[free]
        stub = np.isnan(pop)
        pop[stub] = STUB_FILL * cap[stub]
        return cap, np.round(pop), stub

    work = dcfg["calibration"]["arable"][0]
    dens, cls = variants[work]
    cap, pop, stub = populate(dens, cls)

    # --- checks of §7
    provs = {}
    for k, pid in enumerate(pids):
        f = facts[pid]
        provs[pid] = rules.Province(f.area_km2, TYPES[cls[k]], f.coastal, f.neighbours, f.sea_neighbours, f.zones,
                                    f.irrigated, f.shares)
    comps = rules.components(provs)
    for pid in pids:
        for nb in provs[pid].neighbours:
            assert pid in provs[nb].neighbours
        for nb in provs[pid].sea_neighbours:
            assert pid in provs[nb].sea_neighbours and provs[pid].coastal and provs[nb].coastal
    owners = collections.Counter(provs_cfg[pid]["owner"] for pid in pids)

    # Blocks of the layout's countries left out: holes have all their neighbours inside.
    countries = sorted({c for prov in layout["province"] for part in prov["parts"] for c in part["countries"]})
    left = np.isin(blocks["country"], countries) & (owner_of == "")
    nbrs = collections.defaultdict(set)
    for a, b, _ in edges:
        nbrs[a].add(b)
        nbrs[b].add(a)
    in_layout = set(asg)
    holes = [k for k in np.flatnonzero(left) if nbrs[blocks["id"][k]] and nbrs[blocks["id"][k]] <= in_layout]

    # --- outputs
    write_toml(public / "provinces.toml", args.scenario, layout, provs_cfg, pids, provs, pop, stub, work, rep)
    geoms = write_geojson(public / "provinces.geojson", blocks, owner_of, pids, provs_cfg)
    previews(out, geoms, pids, provs_cfg, provs)

    # --- the measure
    rep("# Раскладка " + args.scenario)
    rep()
    rep(f"Провинций — {len(pids)}, блоков — {len(asg)}, площадь — {area.sum():,.0f} км². Тип земли — по пашне {work:g}; "
        f"рядом — {', '.join(f'{a:g}' for a in variants if a != work)}. Равновесие роста и чумы движка — {balance:.2f}.")
    rep()
    rep("## Приёмка (план §7)")
    rep()
    rep(f"- связность графа по суше и морю: {len(comps)} компонент{'а' if len(comps) == 1 else ''} "
        f"({', '.join(str(len(c)) for c in comps)})")
    rep("- соседство симметрично, морские соседи только у прибрежных — проверено")
    rep(f"- владельцев — {len(owners)}: " + ", ".join(f"{o} {n}" for o, n in sorted(owners.items())))
    e0 = layout["e0_actors"]
    rep("- акторы E0 без провинций: " + (", ".join(a for a in e0 if a not in owners) or "нет"))
    rep("- владельцы вне списка E0 (новые акторы с землёй, решение владельца): " + (", ".join(o for o in sorted(owners) if o not in e0) or "нет"))
    rep(f"- дыр внутри охвата: {len(holes)}" + ("" if not holes else " — " + ", ".join(
        f"{blocks['country'][k]} {blocks['name'][k]}" for k in holes)))
    lt = collections.Counter(TYPES[c] for c in cls)
    rep("- типы земли: " + ", ".join(f"{t} {lt[t]}" for t in TYPES if lt[t]))
    italy = [k for k, pid in enumerate(pids) if plands[pid] is not None]
    rep("- в Италии Desert/Steppe: " + (", ".join(pids[k] for k in italy if cls[k] in (D, S)) or "нет"))
    rep(f"- население — заглушка {STUB_FILL:g} ёмкости (до E4) у провинций без земли с населением: " +
        ", ".join(pids[k] for k in np.flatnonzero(stub)))
    rep()

    rep("## Земли (невязка ёмкости и заполненность на старте)")
    rep()
    rep("| земля | внутри | потолок, чел./км² | провинций | площадь, км² | " +
        " | ".join(f"ёмкость при {a:g}, млн | невязка | заполн." for a in variants) + " |")
    rep("|---|---|---|---|---|" + "---|---|---|" * len(variants))
    for land in sc["lands"]:
        m = member[land]
        row = [land, lands_cfg.get(land, {}).get("within", "—"),
               f"{ceiling[land]:.1f}" if ceiling[land] is not None else "§4 (правило 1)", str(int(m.sum())), f"{area[m].sum():,.0f}"]
        for a, (dv, cv) in variants.items():
            cp, pp, _ = populate(dv, cv)
            c = float(cp[m].sum())
            res = f"{100 * (c / area[m].sum() - ceiling[land]) / ceiling[land]:+.0f} %" if ceiling[land] is not None else "—"
            fl = float(pp[m].sum()) / c
            row += [f"{c / 1e6:.2f}", res, f"{fl:.2f}" + (" — выше" if fl > balance else "")]
        rep("| " + " | ".join(row) + " |")
    rep()
    rep(f"Население на старте: {sc.get('start_population_source', '—')}")
    rep()

    rep("## Провинции")
    rep()
    rep(f"| id | название | владелец | земля | тип при {work:g} | " + " | ".join(f"тип при {a:g}" for a in variants if a != work) +
        " | площадь, км² | изрезано | прибрежная | соседи | по морю | ёмкость, тыс. | население, тыс. | заполн. |")
    rep("|---|---|---|---|---|" + "---|" * (len(variants) - 1) + "---|---|---|---|---|---|---|---|")
    for k, pid in enumerate(pids):
        pc, f = provs_cfg[pid], provs[pid]
        other = " | ".join(TYPES[variants[a][1][k]] for a in variants if a != work)
        fl = pop[k] / cap[k]
        rep(f"| {pid} | {pc['name']} | {pc['owner']} | {plands[pid] or '—'} | {f.land} | {other} | {f.area_km2:,} | "
            f"{100 * rugged[k]:.0f} % | {'да' if f.coastal else 'нет'} | {len(f.neighbours)} | {len(f.sea_neighbours)} | "
            f"{cap[k] / 1e3:,.0f} | {pop[k] / 1e3:,.0f}{' (заглушка)' if stub[k] else ''} | {fl:.2f}{' — выше' if fl > balance else ''} |")
    rep()

    rep("## Морские зоны (план §10a п. 5: разрез — по числу провинций зоны)")
    rep()
    on_zone = collections.defaultdict(list)
    for pid in pids:
        for z in provs[pid].zones:
            on_zone[z].append(pid)
    rep("| зона | провинций | провинции |")
    rep("|---|---|---|")
    for z in sorted(on_zone, key=lambda z: -len(on_zone[z])):
        rep(f"| {z} | {len(on_zone[z])} | {', '.join(on_zone[z])} |")
    rep()

    rep("## Источники и приближения")
    rep()
    for key, text in layout["sources"].items():
        rep(f"- `{key}`: {text}")
    rep()
    for pid in pids:
        if "note" in provs_cfg[pid]:
            rep(f"- **{pid}** ({provs_cfg[pid]['source']}): {provs_cfg[pid]['note']}")
    rep()
    rep("## Блоки охвата, оставленные вне провинций")
    rep()
    by_country = collections.defaultdict(list)
    for k in np.flatnonzero(left):
        by_country[blocks["country"][k]].append(k)
    for c in sorted(by_country):
        ks = by_country[c]
        rep(f"- {c}: {len(ks)} блоков, {blocks['area'][ks].sum():,.0f} км²")
    rep()
    (out / "report.txt").write_text("\n".join(rep.lines) + "\n", encoding="utf-8")
    rep.step("done")


def toml_str(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def write_toml(path, scenario, layout, provs_cfg, pids, provs, pop, stub, work, rep):
    """The province table in the field names of engine2's schema (`ProvinceData`, `RegionData`)."""
    lines = [f"# {scenario}: provinces and regions in the field names of engine2's scenario schema.",
             "# Generated by tools/map/layout.py from tools/map/layouts/" + scenario + ".toml — do not edit by hand.",
             f"# land — the capacity class calibrated at arable {work:g} per km² (docs/engine2_map.md §11a–§11c);",
             "# population — the start population by the rule of §11d; provinces of no land carry a stub",
             f"# ({STUB_FILL:g} of capacity) until E4: " + ", ".join(pid for k, pid in enumerate(pids) if stub[k]) + ".",
             ""]
    for k, pid in enumerate(pids):
        pc, f = provs_cfg[pid], provs[pid]
        lines += [f"[provinces.{pid}]",
                  f"name = {toml_str(pc['name'])}",
                  f"owner = {toml_str(pc['owner'])}",
                  f"area_km2 = {f.area_km2}",
                  f"land = {toml_str(f.land)}",
                  f"coastal = {'true' if f.coastal else 'false'}",
                  f"population = {int(pop[k])}",
                  "neighbours = [" + ", ".join(toml_str(x) for x in f.neighbours) + "]",
                  "sea_neighbours = [" + ", ".join(toml_str(x) for x in f.sea_neighbours) + "]",
                  f"culture = {toml_str(pc['culture'])}",
                  f"religion = {toml_str(pc['religion'])}",
                  ""]
    members = collections.defaultdict(list)
    for pid in pids:
        members[provs_cfg[pid]["region"]].append(pid)
    for region in sorted(members):
        lines += [f"[regions.{region}]",
                  f"name = {toml_str(layout['regions'][region])}",
                  "provinces = [" + ", ".join(toml_str(x) for x in members[region]) + "]",
                  ""]
    path.write_text("\n".join(lines), encoding="utf-8", newline="\n")
    rep.step(f"wrote {path.relative_to(REPO)}")


def write_geojson(path, blocks, owner_of, pids, provs_cfg):
    """Simplified province polygons for the interface (E5): properties id, name, owner, region."""
    geoms = {}
    feats = []
    for pid in pids:
        g = shapely.union_all(blocks["geometry"][owner_of == pid])
        g = shapely.transform(shapely.simplify(g, SIMPLIFY_DEG, preserve_topology=True), lambda c: np.round(c, 4))
        geoms[pid] = g
        pc = provs_cfg[pid]
        feats.append({"type": "Feature",
                      "properties": {"id": pid, "name": pc["name"], "owner": pc["owner"], "region": pc["region"]},
                      "geometry": mapping(g)})
    text = json.dumps({"type": "FeatureCollection", "features": feats}, ensure_ascii=False, separators=(",", ":"))
    path.write_text(text + "\n", encoding="utf-8", newline="\n")
    return geoms


def previews(out, geoms, pids, provs_cfg, provs):
    owners = sorted({provs_cfg[pid]["owner"] for pid in pids})
    palette = list(plt.get_cmap("tab20").colors) + list(plt.get_cmap("Set3").colors)
    colour = {o: palette[(7 * k) % len(palette)] for k, o in enumerate(owners)}
    df = gpd.GeoDataFrame({"id": pids, "owner": [provs_cfg[pid]["owner"] for pid in pids],
                           "land": [provs[pid].land for pid in pids]}, geometry=[geoms[pid] for pid in pids], crs="EPSG:4326")
    land_colour = {"Arable": "#e9d98b", "Hills": "#9c7b52", "Steppe": "#c9b77d", "Desert": "#e8c9a0", "Irrigated": "#5aa86b"}
    for name, col, legend in (("owners", df["owner"].map(colour), owners), ("land", df["land"].map(land_colour), list(land_colour))):
        fig, ax = plt.subplots(figsize=(16, 13), dpi=110)
        df.plot(ax=ax, color=list(col), edgecolor="#333333", linewidth=0.4)
        for pid, g in zip(pids, df.geometry):
            pt = g.representative_point()
            ax.annotate(provs_cfg[pid]["name"], (pt.x, pt.y), ha="center", va="center", fontsize=5.5)
        handles = [plt.Rectangle((0, 0), 1, 1, color=colour[o] if name == "owners" else land_colour[o]) for o in legend]
        ax.legend(handles, legend, loc="lower left", fontsize=7, ncol=2)
        ax.set_aspect(1.35)
        ax.set_axis_off()
        fig.savefig(out / f"preview_{name}.png", bbox_inches="tight")
        plt.close(fig)


if __name__ == "__main__":
    main()
