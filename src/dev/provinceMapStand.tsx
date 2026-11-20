// The stand of the province map (docs/engine2_map.md §37.7): `ProvinceMap` fed with recorded
// inputs in place of the engine's view, so that it can be looked at and checked without Tauri.
// Nothing of the application imports this file.
import React, { useEffect, useState } from 'react';
import ReactDOM from 'react-dom/client';
import { ProvinceMap } from '../components/ProvinceMap';
import type { ProvinceMapActor, ProvinceMapProvince, ProvinceMapUnit } from '../components/ProvinceMap';

interface Recording {
  viewer: string | null;
  provinces: ProvinceMapProvince[];
  actors: ProvinceMapActor[];
  units: ProvinceMapUnit[];
}

const SCENARIO = 'milan_1477';
/** The province the «без провинции» case leaves out of the inputs. */
const LEFT_OUT = 'firenze';

type Case = 'start' | 'after' | 'fallen' | 'missing' | 'broken' | 'old';

const CASES: { id: Case; label: string }[] = [
  { id: 'start', label: 'старт' },
  { id: 'after', label: 'после хода' },
  { id: 'fallen', label: 'держава пала' },
  { id: 'missing', label: 'без провинции' },
  { id: 'broken', label: 'карта не загрузилась' },
  { id: 'old', label: 'карта без точек' },
];

/** The polity the «держава пала» case fells, and who takes its land. */
const FALLEN = 'lucca';
const TAKEN_BY = 'florence';

const Stand: React.FC = () => {
  const [which, setWhich] = useState<Case>('start');
  const [recordings, setRecordings] = useState<Record<'start' | 'after', Recording> | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    Promise.all(
      (['start', 'after'] as const).map((name) =>
        fetch(`/dev/fixtures/${SCENARIO}_${name}.json`).then((r) => r.json() as Promise<Recording>),
      ),
    ).then(([start, after]) => setRecordings({ start, after }));
  }, []);

  if (!recordings) return <div>Записи загружаются…</div>;

  const base = which === 'after' ? recordings.after : recordings.start;
  const view: Recording =
    which === 'missing'
      ? { ...base, provinces: base.provinces.filter((p) => p.id !== LEFT_OUT) }
      : which === 'fallen'
        ? {
            // As the engine's view shows a fall: the land is another's, the polity stays in
            // the list with `alive = false` and the capital it had.
            ...base,
            provinces: base.provinces.map((p) => (p.owner === FALLEN ? { ...p, owner: TAKEN_BY } : p)),
            actors: base.actors.map((a) => (a.id === FALLEN ? { ...a, alive: false } : a)),
          }
        : base;
  const geojsonUrl =
    which === 'broken'
      ? `/geodata2/${SCENARIO}/no-such-file.geojson`
      : which === 'old'
        ? '/dev/fixtures/provinces_without_label.geojson'
        : `/geodata2/${SCENARIO}/provinces.geojson`;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ padding: 6, display: 'flex', gap: 6, alignItems: 'center' }}>
        {CASES.map((c) => (
          <button key={c.id} data-testid={`case-${c.id}`} disabled={c.id === which} onClick={() => setWhich(c.id)}>
            {c.label}
          </button>
        ))}
        <span data-testid="selected">выбрана: {selected ?? '—'}</span>
      </div>
      <div style={{ flex: 1, minHeight: 0 }}>
        <ProvinceMap
          geojsonUrl={geojsonUrl}
          styleUrl={`/geodata2/${SCENARIO}/map_style.json`}
          provinces={view.provinces}
          actors={view.actors}
          units={view.units}
          viewer={view.viewer}
          selectedProvince={selected}
          onSelectProvince={setSelected}
        />
      </div>
    </div>
  );
};

ReactDOM.createRoot(document.getElementById('root')!).render(<Stand />);
