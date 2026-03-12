import React, { useEffect, useState, useRef, useMemo } from 'react';
import { MapContainer, TileLayer, GeoJSON, Tooltip, CircleMarker } from 'react-leaflet';
import type { Map as LeafletMap } from 'leaflet';
import 'leaflet/dist/leaflet.css';
import { getMapConfig, getWorldState } from '../api';
import type { MapConfig, WorldState } from '../types/index';
import type { HeatmapMetric } from '../utils/heatmapColor';
import { HEATMAP_LABELS, metricToHeatmapColor } from '../utils/heatmapColor';
import { computePathStyle } from '../utils/mapStyle';

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

  // Heatmap state
  const [heatmapEnabled, setHeatmapEnabled] = useState(false);
  const [heatmapMetric, setHeatmapMetric] = useState<HeatmapMetric>('cohesion');

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

  // Update all polygon styles when heatmap mode or metric changes
  useEffect(() => {
    if (!mapConfig || !worldState) return;
    mapConfig.polygons.forEach(polygon => {
      const ref = geoJsonRefs.current[polygon.actor_id];
      if (!ref) return;
      const actor = worldState.actors[polygon.actor_id];
      ref.setStyle(computePathStyle({
        polygon,
        actor,
        isFading: fadingOut.has(polygon.actor_id),
        isSelected: selectedActorId === polygon.actor_id,
        isHovered: hoveredActorId === polygon.actor_id,
        heatmapEnabled,
        heatmapMetric,
      }));
    });
  }, [heatmapEnabled, heatmapMetric, worldState?.actors, fadingOut, selectedActorId, hoveredActorId, mapConfig]);

  // Build actor map for quick lookup
  const actorMap = useMemo(
    () => worldState?.actors ?? {},
    [worldState?.actors]
  );

  if (!mapConfig) return null;

  const isAlive = (actorId: string) => actorId in actorMap;

  // Build set of actor IDs that have polygons
  const polygonActorIds = useMemo(() => {
    return new Set(mapConfig.polygons.map(p => p.actor_id));
  }, [mapConfig]);

  // Filter visible polygons: only alive or fading out
  const visiblePolygons = mapConfig.polygons.filter(polygon =>
    isAlive(polygon.actor_id) || fadingOut.has(polygon.actor_id)
  );

  // Find spawned actors: alive, no polygon, has center coordinates
  const spawnedActors = useMemo(() => {
    return Object.values(actorMap).filter(actor =>
      !polygonActorIds.has(actor.id) &&
      actor.center !== null &&
      actor.center !== undefined
    );
  }, [actorMap, polygonActorIds]);

  return (
    <div className="map-panel">
      {/* Controls overlay */}
      <div className="map-controls">
        <button
          className={`map-heatmap-toggle ${heatmapEnabled ? 'active' : ''}`}
          onClick={() => setHeatmapEnabled(v => !v)}
        >
          Тепловая карта
        </button>

        {heatmapEnabled && (
          <div className="map-metric-buttons">
            {(Object.keys(HEATMAP_LABELS) as HeatmapMetric[]).map(metric => (
              <button
                key={metric}
                className={`map-metric-btn ${heatmapMetric === metric ? 'active' : ''}`}
                onClick={() => setHeatmapMetric(metric)}
              >
                {HEATMAP_LABELS[metric]}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Legend overlay */}
      {heatmapEnabled && (
        <div className="map-legend">
          <div className="map-legend-title">{HEATMAP_LABELS[heatmapMetric]}</div>
          <div className="map-legend-scale">
            <span className="map-legend-label">0</span>
            <div className="map-legend-gradient" />
            <span className="map-legend-label">100</span>
          </div>
        </div>
      )}

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
          const actor = actorMap[polygon.actor_id];

          return (
            <GeoJSON
              key={polygon.actor_id}
              // eslint-disable-next-line @typescript-eslint/no-explicit-any
              data={data as any}
              ref={el => { geoJsonRefs.current[polygon.actor_id] = el; }}
              style={computePathStyle({
                polygon,
                actor,
                isFading,
                isSelected: selectedActorId === polygon.actor_id,
                isHovered: hoveredActorId === polygon.actor_id,
                heatmapEnabled,
                heatmapMetric,
              })}
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

        {/* Spawned actor markers (actors without polygons) */}
        {spawnedActors.map((actor) => {
          if (!actor.center) return null;
          
          const isFading = fadingOut.has(actor.id);
          const isSelected = selectedActorId === actor.id;
          const isHovered = hoveredActorId === actor.id;
          
          // Default color for spawned actors
          const baseColor = '#607d8b';
          
          // Heatmap color if enabled
          const fillColor = heatmapEnabled && !isFading
            ? metricToHeatmapColor(heatmapMetric, actor.metrics[heatmapMetric] ?? 50)
            : baseColor;
          
          const borderColor = isFading
            ? baseColor
            : isSelected || isHovered
            ? '#ffffff'
            : baseColor;
          
          // Radius based on military_size
          const militarySize = actor.metrics.military_size ?? 0;
          const radius = 6 + Math.min(10, militarySize / 100 * 10);
          
          const fillOpacity = isFading
            ? 0
            : isSelected
            ? Math.min(0.7, 0.5 + 0.2)
            : isHovered
            ? Math.min(0.6, 0.5 + 0.1)
            : 0.5;

          return (
            <CircleMarker
              key={actor.id}
              center={[actor.center.lat, actor.center.lng]}
              radius={radius}
              fillColor={fillColor}
              color={borderColor}
              fillOpacity={fillOpacity}
              weight={isSelected ? 2 : isHovered ? 1.5 : 1}
              eventHandlers={{
                click: () => {
                  if (!isFading) {
                    onSelectActor(actor.id);
                  }
                },
                mouseover: () => {
                  if (!isFading) {
                    setHoveredActorId(actor.id);
                  }
                },
                mouseout: () => {
                  setHoveredActorId(null);
                },
              }}
            >
              {/* Tooltip: only for alive, non-fading actors */}
              {!isFading && (
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
            </CircleMarker>
          );
        })}
      </MapContainer>
    </div>
  );
};
