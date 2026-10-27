"""The rules of plan §4 on provinces made of blocks. A province's facts are sums over its
blocks: area, rugged and dry shares, borders, coasts by sea area, river length. Plain data in,
plain data out; the geometry is measured once, in base.py."""

from collections import defaultdict
from dataclasses import dataclass, field


@dataclass(frozen=True)
class Params:
    l_min_km: float
    coast_min_km: float
    hills_share: float
    desert_share: float
    steppe_share: float
    irrigated_min_river_km: float

    @staticmethod
    def from_toml(p: dict) -> "Params":
        return Params(
            l_min_km=p["neighbours"]["l_min_km"],
            coast_min_km=p["coast"]["min_km"],
            hills_share=p["land"]["hills_share"],
            desert_share=p["land"]["desert_share"],
            steppe_share=p["land"]["steppe_share"],
            irrigated_min_river_km=p["irrigated"]["min_river_km"],
        )


@dataclass(frozen=True)
class Block:
    area_km2: float      # of the polygon, equal-area projection
    raster_km2: float    # of the relief cells counted in the block
    rugged_km2: float
    climate_km2: float   # of the climate cells with a class (sea cells excluded)
    bw_km2: float
    bs_km2: float
    river_km: float      # of major rivers


@dataclass
class Province:
    area_km2: int
    land: str
    coastal: bool
    neighbours: list[str]
    sea_neighbours: list[str]
    zones: list[str]
    irrigated: bool
    shares: dict[str, float] = field(default_factory=dict)


def land_type(rugged: float, bw: float, bs: float, p: Params) -> str:
    """Closed list of engine2 (`Land`), in the order of plan §4."""
    if rugged >= p.hills_share:
        return "Hills"
    if bw >= p.desert_share:
        return "Desert"
    if bw + bs >= p.steppe_share:
        return "Steppe"
    return "Arable"


def _share(part: float, whole: float) -> float:
    return part / whole if whole > 0 else 0.0


def provinces(assign: dict[str, str], blocks: dict[str, Block],
              edges: list[tuple[str, str, float]], coast: list[tuple[str, str, float]],
              p: Params) -> dict[str, Province]:
    """Facts of the provinces that `assign` (block → province) makes of the blocks."""
    members: dict[str, list[str]] = defaultdict(list)
    for b, prov in assign.items():
        members[prov].append(b)

    border: dict[tuple[str, str], float] = defaultdict(float)
    for a, b, km in edges:
        pa, pb = assign.get(a), assign.get(b)
        if pa is None or pb is None or pa == pb:
            continue
        border[(min(pa, pb), max(pa, pb))] += km
    land_nb: dict[str, set[str]] = defaultdict(set)
    for (pa, pb), km in border.items():
        if km >= p.l_min_km:
            land_nb[pa].add(pb)
            land_nb[pb].add(pa)

    shore: dict[str, dict[str, float]] = defaultdict(lambda: defaultdict(float))
    for b, zone, km in coast:
        prov = assign.get(b)
        if prov is not None:
            shore[prov][zone] += km
    zones = {prov: sorted(z for z, km in shore[prov].items() if km >= p.coast_min_km)
             for prov in members}
    coastal = {prov: sum(shore[prov].values()) >= p.coast_min_km and bool(zones[prov])
               for prov in members}
    on_zone: dict[str, list[str]] = defaultdict(list)
    for prov in sorted(members):
        if coastal[prov]:
            for z in zones[prov]:
                on_zone[z].append(prov)
    sea_nb: dict[str, set[str]] = defaultdict(set)
    for provs in on_zone.values():
        for x in provs:
            sea_nb[x].update(y for y in provs if y != x)

    out = {}
    for prov in sorted(members):
        bs = [blocks[b] for b in members[prov]]
        raster = sum(x.raster_km2 for x in bs)
        climate = sum(x.climate_km2 for x in bs)
        rugged = _share(sum(x.rugged_km2 for x in bs), raster)
        bw = _share(sum(x.bw_km2 for x in bs), climate)
        bsh = _share(sum(x.bs_km2 for x in bs), climate)
        land = land_type(rugged, bw, bsh, p)
        river = sum(x.river_km for x in bs)
        out[prov] = Province(
            area_km2=round(sum(x.area_km2 for x in bs)),
            land=land,
            coastal=coastal[prov],
            neighbours=sorted(land_nb[prov]),
            sea_neighbours=sorted(sea_nb[prov]) if coastal[prov] else [],
            zones=zones[prov] if coastal[prov] else [],
            irrigated=land in ("Desert", "Steppe") and river >= p.irrigated_min_river_km,
            shares={"rugged": rugged, "bw": bw, "bs": bsh},
        )
    return out


def components(provs: dict[str, Province]) -> list[list[str]]:
    """Connected components of the land-and-sea graph, largest first."""
    seen: set[str] = set()
    comps = []
    for start in sorted(provs):
        if start in seen:
            continue
        stack, comp = [start], []
        seen.add(start)
        while stack:
            x = stack.pop()
            comp.append(x)
            for y in provs[x].neighbours + provs[x].sea_neighbours:
                if y not in seen:
                    seen.add(y)
                    stack.append(y)
        comps.append(sorted(comp))
    comps.sort(key=lambda c: (-len(c), c[0]))
    return comps
