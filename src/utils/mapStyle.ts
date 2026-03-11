import type { MapPolygon } from '../types';
import type { Actor } from '../types';
import type { PathOptions } from 'leaflet';
import { metricToHeatmapColor, HeatmapMetric } from './heatmapColor';

interface PathStyleOptions {
  polygon: MapPolygon;
  actor: Actor | undefined;
  isFading: boolean;
  isSelected: boolean;
  isHovered: boolean;
  heatmapEnabled: boolean;
  heatmapMetric: HeatmapMetric;
}

export function computePathStyle(opts: PathStyleOptions): PathOptions {
  const { polygon, actor, isFading, isSelected, isHovered, heatmapEnabled, heatmapMetric } = opts;

  // Fading: всё обнуляется, heatmap не пересчитывается
  if (isFading) {
    return { fillOpacity: 0, opacity: 0, weight: 0 };
  }

  const baseFillColor = heatmapEnabled && actor
    ? metricToHeatmapColor(heatmapMetric, actor.metrics[heatmapMetric] ?? 50)
    : polygon.color;

  // В heatmap border чуть темнее fill, но не совпадает с ним
  const borderColor = (isSelected || isHovered)
    ? '#ffffff'
    : heatmapEnabled
    ? 'rgba(0,0,0,0.35)'
    : polygon.color;

  const fillOpacity = isSelected
    ? Math.min(polygon.opacity + 0.2, 1.0)
    : isHovered
    ? Math.min(polygon.opacity + 0.1, 1.0)
    : polygon.opacity;

  const weight = isSelected ? 2 : isHovered ? 1.5 : 1;

  return {
    fillColor: baseFillColor,
    color: borderColor,
    fillOpacity,
    opacity: 1,
    weight,
  };
}
