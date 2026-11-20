// The province map of engine2 (docs/engine2_map.md §37): one GeoJSON of the scenario's
// provinces, coloured by the owner the engine's view gives, with capitals and units as markers.
//
// It is a second path of data beside the old `MapPanel` (AGENTS §11), which it does not touch:
// no `MapConfig`, no `public/geodata/`, no tiles from the network. It takes props only and
// computes nothing about the world (AGENTS §12): what is not in its props is not on the map.
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { GeoJSON, MapContainer, Marker } from 'react-leaflet';
import L from 'leaflet';
import type { GeoJSON as LeafletGeoJSON, Layer, Path, PathOptions } from 'leaflet';
import type { GeoJsonObject } from 'geojson';
import 'leaflet/dist/leaflet.css';
import './ProvinceMap.css';
import { boundsOfFeatures, colourOfActor, markerOffset } from '../utils/provinceMap';
import type { ProvinceMapStyle } from '../utils/provinceMap';

export interface ProvinceMapProvince {
  id: string;
  owner: string;
}

/**
 * A polity as the engine's view names it. A fallen polity stays in the view with
 * `alive = false` and the capital it had: its mark is not drawn — the province is another's
 * now. The map reads the flag the view gives and decides nothing about who is alive.
 */
export interface ProvinceMapActor {
  id: string;
  name: string;
  alive: boolean;
  /** The province of the capital, by its id; `null` — none shown. */
  capital: string | null;
}

/** A unit as the engine's view names it: `position` is the id of the province it stands in. */
export interface ProvinceMapUnit {
  id: string;
  owner: string;
  position: string;
}

export interface ProvinceMapProps {
  /** The scenario's provinces: `/geodata2/<scenario>/provinces.geojson`. */
  geojsonUrl: string;
  /** The scenario's colours: `/geodata2/<scenario>/map_style.json`. */
  styleUrl: string;
  provinces: ProvinceMapProvince[];
  actors: ProvinceMapActor[];
  units: ProvinceMapUnit[];
  /** The polity the player sees the world as; its land is outlined. */
  viewer: string | null;
  selectedProvince: string | null;
  onSelectProvince: (id: string) => void;
}

interface ProvinceFeature {
  type: 'Feature';
  properties: { id: string; name: string; label: [number, number] };
  geometry: { type: string; coordinates: unknown };
}

interface ProvinceCollection {
  type: 'FeatureCollection';
  features: ProvinceFeature[];
}

type Loaded =
  | { state: 'loading' }
  | { state: 'failed'; why: string }
  | { state: 'ready'; geo: ProvinceCollection; style: ProvinceMapStyle };

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  try {
    return (await res.json()) as T;
  } catch {
    throw new Error(`${url}: не JSON`);
  }
}

/**
 * A map the component can draw: every province with its id and its point. A file without
 * them — an older file, another scenario's — is refused as a whole, with the reason, and not
 * drawn in part (AGENTS §11: no stand-in map).
 */
function checkedProvinces(raw: unknown, url: string): ProvinceCollection {
  const features = (raw as { features?: unknown }).features;
  if (!Array.isArray(features) || features.length === 0) throw new Error(`${url}: в файле нет провинций`);
  features.forEach((f, i) => {
    const p = (f as { properties?: { id?: unknown; label?: unknown } }).properties;
    if (typeof p?.id !== 'string') throw new Error(`${url}: у объекта ${i} нет id`);
    const label = p.label;
    if (!Array.isArray(label) || label.length !== 2 || !label.every((x) => typeof x === 'number')) {
      throw new Error(`${url}: у провинции ${p.id} нет точки label`);
    }
  });
  return raw as ProvinceCollection;
}

const UNIT_STEP_PX = 14;

function unitIcon(colour: string, dx: number, dy: number): L.DivIcon {
  return L.divIcon({
    className: 'pm-unit',
    html: `<span class="pm-unit-mark" style="background:${colour}"></span>`,
    iconSize: [12, 12],
    // The anchor is the point of the icon that stands on the place: moving it by −dx moves
    // the icon by +dx.
    iconAnchor: [6 - dx, 6 - dy],
  });
}

function capitalIcon(colour: string): L.DivIcon {
  return L.divIcon({
    className: 'pm-capital',
    html: `<span class="pm-capital-mark" style="background:${colour}"></span>`,
    iconSize: [10, 10],
    iconAnchor: [5, 5],
  });
}

export const ProvinceMap: React.FC<ProvinceMapProps> = ({
  geojsonUrl,
  styleUrl,
  provinces,
  actors,
  units,
  viewer,
  selectedProvince,
  onSelectProvince,
}) => {
  const [loaded, setLoaded] = useState<Loaded>({ state: 'loading' });
  const [hovered, setHovered] = useState<string | null>(null);
  const layerRef = useRef<LeafletGeoJSON | null>(null);
  const selectRef = useRef(onSelectProvince);
  selectRef.current = onSelectProvince;

  useEffect(() => {
    let live = true;
    setLoaded({ state: 'loading' });
    Promise.all([fetchJson<unknown>(geojsonUrl), fetchJson<ProvinceMapStyle>(styleUrl)])
      .then(([raw, style]) => {
        const geo = checkedProvinces(raw, geojsonUrl);
        if (live) setLoaded({ state: 'ready', geo, style });
      })
      .catch((e: unknown) => {
        // No stand-in map (AGENTS §11): the failure is shown.
        if (live) setLoaded({ state: 'failed', why: e instanceof Error ? e.message : String(e) });
      });
    return () => {
      live = false;
    };
  }, [geojsonUrl, styleUrl]);

  const ownerOf = useMemo(() => new Map(provinces.map((p) => [p.id, p.owner])), [provinces]);
  const ownerRef = useRef(ownerOf);
  ownerRef.current = ownerOf;
  const actorOf = useMemo(() => new Map(actors.map((a) => [a.id, a])), [actors]);

  const geo = loaded.state === 'ready' ? loaded.geo : null;
  const style = loaded.state === 'ready' ? loaded.style : null;

  const featureOf = useMemo(
    () => new Map((geo?.features ?? []).map((f) => [f.properties.id, f])),
    [geo],
  );
  const bounds = useMemo(() => (geo ? boundsOfFeatures(geo.features) : null), [geo]);

  // What the view names and the map does not have, and the reverse: shown, never hidden.
  const unknownToMap = useMemo(
    () => (geo ? provinces.filter((p) => !featureOf.has(p.id)).map((p) => p.id) : []),
    [geo, provinces, featureOf],
  );
  const unitsOffMap = useMemo(
    () => (geo ? units.filter((u) => !featureOf.has(u.position)).map((u) => u.id) : []),
    [geo, units, featureOf],
  );

  const pathStyle = useMemo(() => {
    if (!style) return undefined;
    return (feature?: { properties: { id: string } }): PathOptions => {
      const id = feature?.properties.id ?? '';
      const owner = ownerOf.get(id);
      const selected = id === selectedProvince;
      const own = owner !== undefined && owner === viewer;
      return {
        // A province the view does not name is hatched (the class below), not painted with
        // the owner of the start the GeoJSON carries.
        fillColor: owner === undefined ? style.no_data : colourOfActor(owner, style),
        fillOpacity: 1,
        color: selected ? '#ffffff' : own ? '#111111' : style.border,
        weight: selected ? 3 : own ? 1.6 : 0.6,
      };
    };
  }, [style, ownerOf, viewer, selectedProvince]);

  // The class of «no data» and the province's id on its path: a class is set when the path
  // is made and `setStyle` does not move it, so it is kept by hand — when the path is added
  // to the map (`decorate` from the layer's `add`) and whenever the owners change.
  const decorate = (l: Layer) => {
    const path = l as Path & { feature?: { properties: { id: string } } };
    const el = path.getElement();
    const id = path.feature?.properties.id;
    if (!el || id === undefined) return;
    el.setAttribute('data-province', id);
    el.classList.toggle('pm-nodata', !ownerRef.current.has(id));
  };
  const decorateRef = useRef(decorate);
  decorateRef.current = decorate;
  useEffect(() => {
    const layer = layerRef.current;
    if (!layer) return;
    layer.eachLayer((l: Layer) => {
      decorateRef.current(l);
      const path = l as Path & { feature?: { properties: { id: string } } };
      if (path.feature?.properties.id === selectedProvince && path.getElement()) path.bringToFront();
    });
  }, [ownerOf, selectedProvince]);

  if (loaded.state === 'loading') {
    return (
      <div className="pm-root pm-message" data-testid="pm-loading">
        Карта загружается…
      </div>
    );
  }
  if (loaded.state === 'failed' || !geo || !style || !bounds) {
    return (
      <div className="pm-root pm-message pm-error" data-testid="pm-error">
        Карта не загружена: {loaded.state === 'failed' ? loaded.why : 'в файле нет провинций'}
      </div>
    );
  }

  const unitsIn = new Map<string, ProvinceMapUnit[]>();
  for (const u of units) {
    if (!featureOf.has(u.position)) continue;
    unitsIn.set(u.position, [...(unitsIn.get(u.position) ?? []), u]);
  }
  const capitals = actors.filter((a) => a.alive && a.capital !== null && featureOf.has(a.capital));
  const hoveredFeature = hovered ? featureOf.get(hovered) : undefined;
  const hoveredOwner = hovered ? ownerOf.get(hovered) : undefined;

  return (
    <div className="pm-root" data-testid="pm-root">
      <svg className="pm-defs" aria-hidden="true">
        <defs>
          <pattern id="pm-hatch" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
            <rect width="6" height="6" fill={style.no_data} />
            <line x1="0" y1="0" x2="0" y2="6" stroke="#ffffff" strokeWidth="2" />
          </pattern>
        </defs>
      </svg>
      <MapContainer
        bounds={bounds}
        boundsOptions={{ padding: [12, 12] }}
        zoomSnap={0.25}
        attributionControl={false}
        style={{ height: '100%', width: '100%', background: style.sea }}
      >
        <GeoJSON
          key={geojsonUrl}
          ref={layerRef}
          data={geo as unknown as GeoJsonObject}
          style={pathStyle as L.StyleFunction}
          onEachFeature={(feature, layer) => {
            const id = (feature as unknown as ProvinceFeature).properties.id;
            (layer as Path).options.className = 'pm-province';
            layer.on({
              add: () => decorateRef.current(layer),
              click: () => selectRef.current(id),
              mouseover: () => setHovered(id),
              mouseout: () => setHovered((h) => (h === id ? null : h)),
            });
          }}
        />
        {capitals.map((a) => {
          const [lon, lat] = featureOf.get(a.capital as string)!.properties.label;
          return (
            <Marker
              key={`capital-${a.id}`}
              position={[lat, lon]}
              icon={capitalIcon(colourOfActor(a.id, style))}
              interactive={false}
              keyboard={false}
            />
          );
        })}
        {[...unitsIn.entries()].flatMap(([province, here]) => {
          const [lon, lat] = featureOf.get(province)!.properties.label;
          // In a capital the units stand below its mark, so that both are seen; elsewhere on
          // the province's own point — a shift would carry them into a small neighbour.
          const below = capitals.some((a) => a.capital === province) ? 12 : 0;
          return here.map((u, k) => {
            const [dx, dy] = markerOffset(k, here.length, UNIT_STEP_PX);
            return (
              <Marker
                key={`unit-${u.id}`}
                position={[lat, lon]}
                icon={unitIcon(colourOfActor(u.owner, style), dx, dy + below)}
                interactive={false}
                keyboard={false}
              />
            );
          });
        })}
      </MapContainer>
      {hoveredFeature && (
        <div className="pm-info" data-testid="pm-info">
          <strong>{hoveredFeature.properties.name}</strong>
          <span>
            {hoveredOwner === undefined ? 'нет данных' : (actorOf.get(hoveredOwner)?.name ?? hoveredOwner)}
          </span>
        </div>
      )}
      {(unknownToMap.length > 0 || unitsOffMap.length > 0) && (
        <div className="pm-warning" data-testid="pm-warning">
          {unknownToMap.length > 0 && <div>Нет на карте провинций: {unknownToMap.join(', ')}</div>}
          {unitsOffMap.length > 0 && <div>Отряды в провинциях вне карты: {unitsOffMap.join(', ')}</div>}
        </div>
      )}
    </div>
  );
};
