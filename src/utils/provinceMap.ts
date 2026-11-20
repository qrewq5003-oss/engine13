// Pure helpers of the province map of engine2 (docs/engine2_map.md §37). Nothing here reads
// the world: the owner of a province comes from the view the component is given.

/** The style of a scenario's map: `public/geodata2/<scenario>/map_style.json`. */
export interface ProvinceMapStyle {
  sea: string;
  border: string;
  no_data: string;
  /** Colour of a polity of the start, by its scenario id. */
  actors: Record<string, string>;
}

/** FNV-1a over the id: a stable number for an id, the same in every run. */
function hashOf(id: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < id.length; i += 1) {
    h ^= id.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function hslToHex(h: number, s: number, l: number): string {
  const a = s * Math.min(l, 1 - l);
  const channel = (n: number) => {
    const k = (n + h / 30) % 12;
    const v = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(255 * v)
      .toString(16)
      .padStart(2, '0');
  };
  return `#${channel(0)}${channel(8)}${channel(4)}`;
}

/** A colour for a polity the style does not name — one born in play: from its id alone. */
export function colourFromId(id: string): string {
  const h = hashOf(id);
  return hslToHex(h % 360, 0.45 + ((h >>> 9) % 20) / 100, 0.5 + ((h >>> 17) % 16) / 100);
}

/** The colour of a polity: the scenario's own, else the one its id gives. */
export function colourOfActor(id: string, style: ProvinceMapStyle): string {
  return style.actors[id] ?? colourFromId(id);
}

type Position = number[];

function eachPosition(coordinates: unknown, visit: (p: Position) => void): void {
  if (!Array.isArray(coordinates)) return;
  if (typeof coordinates[0] === 'number') {
    visit(coordinates as Position);
    return;
  }
  for (const inner of coordinates) eachPosition(inner, visit);
}

/**
 * The frame of the map: the extent of the provinces themselves, as Leaflet's
 * [[south, west], [north, east]]. No centre and no zoom are stored anywhere (§37.5).
 */
export function boundsOfFeatures(
  features: { geometry: { coordinates: unknown } }[],
): [[number, number], [number, number]] | null {
  let west = Infinity;
  let south = Infinity;
  let east = -Infinity;
  let north = -Infinity;
  for (const f of features) {
    eachPosition(f.geometry.coordinates, ([lon, lat]) => {
      west = Math.min(west, lon);
      east = Math.max(east, lon);
      south = Math.min(south, lat);
      north = Math.max(north, lat);
    });
  }
  return Number.isFinite(west) ? [[south, west], [north, east]] : null;
}

/**
 * Where the k-th of n markers in one province stands, in pixels from the province's point:
 * one stands on it, several stand in a row centred on it.
 */
export function markerOffset(k: number, n: number, step: number): [number, number] {
  return [(k - (n - 1) / 2) * step, 0];
}
