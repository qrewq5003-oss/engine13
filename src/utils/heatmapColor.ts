export type HeatmapMetric = 'cohesion' | 'legitimacy' | 'economic_output';

export const HEATMAP_LABELS: Record<HeatmapMetric, string> = {
  cohesion: 'Сплочённость',
  legitimacy: 'Легитимность',
  economic_output: 'Экономика',
};

// Все три метрики живут в 0..100 — нормализация не нужна
export function normalizeHeatmapValue(_metric: HeatmapMetric, raw: number): number {
  return Math.max(0, Math.min(100, raw));
}

// Чистая функция: value 0..100 → css color string
export function heatmapValueToColor(value: number): string {
  const v = Math.max(0, Math.min(100, value));
  let r: number, g: number;
  if (v < 50) {
    r = 200;
    g = Math.round((v / 50) * 200);
  } else {
    r = Math.round(((100 - v) / 50) * 200);
    g = 200;
  }
  return `rgb(${r},${g},40)`;
}

export function metricToHeatmapColor(metric: HeatmapMetric, raw: number): string {
  return heatmapValueToColor(normalizeHeatmapValue(metric, raw));
}
