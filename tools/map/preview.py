"""Pictures of the base for the owner's eye (plan §6, §7): blocks coloured by the land type
the rules of §4 give each block alone, and coastal blocks coloured by the sea area they face.

    python -I -X utf8 tools/map/preview.py [--cache D:/map-cache] [--name all] [--bbox LON0 LAT0 LON1 LAT1]
"""

import argparse
import csv
import pathlib
import sys
import tomllib

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import geopandas as gpd  # noqa: E402
import matplotlib  # noqa: E402

matplotlib.use("Agg")
import matplotlib.patches  # noqa: E402
import matplotlib.pyplot as plt  # noqa: E402

import rules  # noqa: E402

LAND_COLOURS = {"Arable": "#9cc37a", "Hills": "#a8825a", "Steppe": "#e3cf7a", "Desert": "#f1e6c4"}
ZONE_PALETTE = ["#1f77b4", "#ff7f0e", "#2ca02c", "#d62728", "#9467bd", "#8c564b", "#e377c2", "#7f7f7f",
                "#bcbd22", "#17becf", "#393b79", "#637939", "#8c6d31", "#843c39", "#7b4173", "#3182bd",
                "#e6550d", "#31a354", "#756bb1", "#636363"]


def read_base(base: pathlib.Path):
    with (base / "blocks.csv").open(encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    blocks = {r["id"]: rules.Block(*(float(r[k]) for k in ("area_km2", "raster_km2", "rugged_km2", "climate_km2",
                                                            "bw_km2", "bs_km2", "river_km"))) for r in rows}
    with (base / "borders.csv").open(encoding="utf-8") as f:
        edges = [(r["a"], r["b"], float(r["km"])) for r in csv.DictReader(f)]
    with (base / "coasts.csv").open(encoding="utf-8") as f:
        coast = [(r["block"], r["zone"], float(r["km"])) for r in csv.DictReader(f)]
    return blocks, edges, coast


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default="D:/map-cache")
    ap.add_argument("--name", default="all")
    ap.add_argument("--bbox", type=float, nargs=4)
    args = ap.parse_args()
    base = pathlib.Path(args.cache) / "out" / "base"
    params = tomllib.loads((HERE / "params.toml").read_text(encoding="utf-8"))
    blocks, edges, coast = read_base(base)
    provs = rules.provinces({b: b for b in blocks}, blocks, edges, coast, rules.Params.from_toml(params))
    gdf = gpd.read_file(base / "blocks.gpkg", layer="blocks")
    gdf["land"] = [provs[b].land for b in gdf["id"]]
    gdf["irrigated"] = [provs[b].irrigated for b in gdf["id"]]
    # Any coast at all: the 5 km minimum is a rule for provinces, and a Cypriot village of a
    # few km² has less on its own.
    main_zone = {}
    for b, z, km in sorted(coast, key=lambda r: (r[0], -r[2], r[1])):
        main_zone.setdefault(b, z)
    gdf["zone"] = [main_zone.get(b) for b in gdf["id"]]
    if args.bbox:
        x0, y0, x1, y1 = args.bbox
        gdf = gdf.cx[x0:x1, y0:y1]

    fig, ax = plt.subplots(figsize=(18, 11))
    for land, colour in LAND_COLOURS.items():
        part = gdf[gdf.land == land]
        if len(part):
            part.plot(ax=ax, color=colour, edgecolor="#555555", linewidth=0.08)
    irr = gdf[gdf.irrigated]
    if len(irr):
        irr.plot(ax=ax, facecolor="none", edgecolor="#1f5fbf", linewidth=0.6, hatch="////")
    handles = [matplotlib.patches.Patch(color=c, label=k) for k, c in LAND_COLOURS.items()]
    handles.append(matplotlib.patches.Patch(facecolor="none", edgecolor="#1f5fbf", hatch="////", label="орошаемая долина (флаг)"))
    ax.legend(handles=handles, loc="lower left")
    ax.set_title("Тип земли по правилам §4, каждый блок отдельно")
    ax.set_aspect(1.3)
    fig.savefig(base / f"preview_land_{args.name}.png", dpi=150, bbox_inches="tight")
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(18, 11))
    gdf[gdf.zone.isna()].plot(ax=ax, color="#eeeeee", edgecolor="#bbbbbb", linewidth=0.08)
    zones = sorted(z for z in gdf.zone.dropna().unique())
    for k, z in enumerate(zones):
        part = gdf[gdf.zone == z]
        part.plot(ax=ax, color=ZONE_PALETTE[k % len(ZONE_PALETTE)], edgecolor="#555555", linewidth=0.08)
        pt = part.geometry.union_all().representative_point()
        ax.annotate(z, (pt.x, pt.y), fontsize=6, ha="center",
                    bbox=dict(boxstyle="round,pad=0.15", fc="white", ec="none", alpha=0.7))
    ax.set_title("Блоки с берегом по морской зоне (зона с самым длинным берегом блока)")
    ax.set_aspect(1.3)
    fig.savefig(base / f"preview_zones_{args.name}.png", dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(base / f"preview_land_{args.name}.png")
    print(base / f"preview_zones_{args.name}.png")


if __name__ == "__main__":
    main()
