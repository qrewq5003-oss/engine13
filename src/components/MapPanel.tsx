import React, { useEffect, useState, useRef } from 'react';
import { MapContainer, TileLayer, GeoJSON } from 'react-leaflet';
import type { Map as LeafletMap } from 'leaflet';
import 'leaflet/dist/leaflet.css';
import { getMapConfig } from '../api';
import type { MapConfig } from '../types/index';

interface MapPanelProps {
  selectedActorId: string | null;
  onSelectActor: (id: string) => void;
  scenarioId: string;
}

export const MapPanel: React.FC<MapPanelProps> = ({
  selectedActorId,
  onSelectActor,
  scenarioId,
}) => {
  const [mapConfig, setMapConfig] = useState<MapConfig | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [geoJsonData, setGeoJsonData] = useState<Record<string, any>>({});
  const mapRef = useRef<LeafletMap | null>(null);

  useEffect(() => {
    getMapConfig().then((config) => {
      if (!config) return;
      setMapConfig(config);

      // Load all GeoJSON - graceful: error in one polygon doesn't break the map
      Promise.all(
        config.polygons.map(async (polygon) => {
          const url = `/geodata/${config.geojson_base_path}/${polygon.geojson_file}`;
          try {
            const res = await fetch(url);
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            const data = await res.json();
            return [polygon.actor_id, data] as [string, unknown];
          } catch (err) {
            console.error(`MapPanel: failed to load ${url}:`, err);
            return null;
          }
        })
      ).then((entries) => {
        const loaded = Object.fromEntries(
          entries.filter((e): e is [string, unknown] => e !== null)
        );
        setGeoJsonData(loaded);

        // invalidateSize after data loads - Leaflet in flex-layout
        setTimeout(() => mapRef.current?.invalidateSize(), 100);
      });
    });
  }, [scenarioId]);

  if (!mapConfig) return null; // No config - don't render anything

  return (
    <div className="map-panel">
      <MapContainer
        center={[mapConfig.center_lat, mapConfig.center_lon]}
        zoom={mapConfig.default_zoom}
        style={{ height: '100%', width: '100%' }}
        ref={mapRef}
      >
        <TileLayer
          url={mapConfig.tile_url}
          attribution={mapConfig.tile_attribution}
        />
        {mapConfig.polygons.map((polygon) => {
          const data = geoJsonData[polygon.actor_id];
          if (!data) return null;
          const isSelected = selectedActorId === polygon.actor_id;

          return (
            <GeoJSON
              key={`${polygon.actor_id}-${isSelected}`}
              // eslint-disable-next-line @typescript-eslint/no-explicit-any
              data={data as any}
              style={{
                color: polygon.color,
                fillColor: polygon.color,
                fillOpacity: isSelected ? Math.min(polygon.opacity + 0.2, 1.0) : polygon.opacity,
                weight: isSelected ? 2 : 1,
              }}
              eventHandlers={{
                click: () => onSelectActor(polygon.actor_id),
              }}
            />
          );
        })}
      </MapContainer>
    </div>
  );
};
