import React, { useEffect, useState, useRef, useMemo } from 'react';
import { MapContainer, TileLayer, GeoJSON, Tooltip } from 'react-leaflet';
import type { Map as LeafletMap } from 'leaflet';
import 'leaflet/dist/leaflet.css';
import { getMapConfig, getWorldState } from '../api';
import type { MapConfig, WorldState } from '../types/index';

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
  const [worldState, setWorldState] = useState<WorldState | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [geoJsonData, setGeoJsonData] = useState<Record<string, any>>({});
  const mapRef = useRef<LeafletMap | null>(null);
  
  // Fading out actors (for collapse animation)
  const [fadingOut, setFadingOut] = useState<Set<string>>(new Set());
  
  // Hover state
  const [hoveredActorId, setHoveredActorId] = useState<string | null>(null);
  
  // Per-polygon GeoJSON refs for fade-out styling
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const geoJsonRefs = useRef<Record<string, L.GeoJSON | null>>({});
  
  // Track previous actor IDs to detect collapses
  const prevActorIds = useRef<Set<string>>(new Set());

  // Tooltip labels for actor metrics
  const tooltipLabel: Record<string, string> = {
    cohesion: 'Сплочённость',
    legitimacy: 'Легитимность',
    military_size: 'Армия',
    economic_output: 'Экономика',
  };

  // Load map config
  useEffect(() => {
    getMapConfig().then((config) => {
      if (!config) return;
      setMapConfig(config);

      // Load all GeoJSON
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

  // Load world state
  useEffect(() => {
    getWorldState().then((state) => {
      if (state) setWorldState(state);
    });
  }, [scenarioId]);

  // Track actor ID changes for fade-out effect
  useEffect(() => {
    if (!worldState) return;
    
    const currentIds = new Set(Object.keys(worldState.actors));

    prevActorIds.current.forEach(id => {
      if (!currentIds.has(id)) {
        // Actor collapsed - start fade out
        setFadingOut(prev => {
          const next = new Set(prev);
          next.add(id);
          return next;
        });
        
        // Apply fade-out style immediately via ref
        const ref = geoJsonRefs.current[id];
        if (ref) {
          ref.setStyle({ fillOpacity: 0, opacity: 0 });
        }
        
        // Remove from fading set after animation
        setTimeout(() => {
          setFadingOut(prev => {
            const next = new Set(prev);
            next.delete(id);
            return next;
          });
        }, 800);
      }
    });

    prevActorIds.current = currentIds;
  }, [worldState?.actors]);

  // Reset selection when selected actor collapses
  useEffect(() => {
    if (selectedActorId && worldState && !(selectedActorId in worldState.actors)) {
      onSelectActor('');
    }
  }, [worldState?.actors, selectedActorId, onSelectActor]);

  if (!mapConfig) return null;

  // Build actor map for quick lookup
  const actorMap = useMemo(
    () => worldState?.actors ?? {},
    [worldState?.actors]
  );

  const isAlive = (actorId: string) => actorId in actorMap;

  // Filter visible polygons: only alive or fading out
  const visiblePolygons = mapConfig.polygons.filter(polygon =>
    isAlive(polygon.actor_id) || fadingOut.has(polygon.actor_id)
  );

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
        {visiblePolygons.map((polygon) => {
          const data = geoJsonData[polygon.actor_id];
          if (!data) return null;
          
          const isFading = fadingOut.has(polygon.actor_id);
          const isSelected = selectedActorId === polygon.actor_id;
          const isHovered = hoveredActorId === polygon.actor_id;
          
          // Priority: fading > selected > hovered > normal
          // Fading: no hover, no tooltip, opacity 0
          // Selected: opacity +0.2, weight 2, white border
          // Hovered: opacity +0.1, weight 1.5, white border
          // Normal: base opacity, weight 1
          const fillOpacity = isFading
            ? 0
            : isSelected
            ? Math.min(polygon.opacity + 0.2, 1.0)
            : isHovered
            ? Math.min(polygon.opacity + 0.1, 1.0)
            : polygon.opacity;

          const weight = isFading ? 0 : isSelected ? 2 : isHovered ? 1.5 : 1;
          const borderColor = isFading ? polygon.color : (isSelected || isHovered) ? '#ffffff' : polygon.color;

          const actor = actorMap[polygon.actor_id];

          return (
            <GeoJSON
              key={polygon.actor_id}
              // eslint-disable-next-line @typescript-eslint/no-explicit-any
              data={data as any}
              ref={el => { geoJsonRefs.current[polygon.actor_id] = el; }}
              style={{
                color: borderColor,
                fillColor: polygon.color,
                fillOpacity,
                weight,
              }}
              eventHandlers={{
                click: () => {
                  if (!isFading) {
                    onSelectActor(polygon.actor_id);
                  }
                },
                mouseover: () => {
                  if (!isFading) {
                    setHoveredActorId(polygon.actor_id);
                  }
                },
                mouseout: () => {
                  setHoveredActorId(null);
                },
              }}
            >
              {/* Tooltip: only for alive, non-fading actors */}
              {actor && !isFading && (
                <Tooltip sticky>
                  <div className="map-tooltip">
                    <strong>{actor.name}</strong>
                    {(['cohesion', 'legitimacy', 'military_size', 'economic_output'] as const).map(key => {
                      const val = actor.metrics[key];
                      if (val === undefined) return null;
                      return (
                        <div key={key} className="map-tooltip-row">
                          <span className="map-tooltip-key">{tooltipLabel[key]}</span>
                          <span>{val.toFixed(1)}</span>
                        </div>
                      );
                    })}
                  </div>
                </Tooltip>
              )}
            </GeoJSON>
          );
        })}
      </MapContainer>
    </div>
  );
};
