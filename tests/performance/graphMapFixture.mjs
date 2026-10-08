const rng = (seed) => {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};

const DAY = 86400000;

export const MAP_CATEGORIES = [
  { id: 1, title: 'Alarms', code: 'TM', weight: 0.2, hub: 399 },
  { id: 2, title: 'Desktop', code: 'TM', weight: 0.22, hub: 385 },
  { id: 3, title: 'Plugins', code: 'TM', weight: 0.15, hub: 408 },
  { id: 4, title: 'Agents & personas', code: 'TM', weight: 0.2, hub: 389 },
  { id: 5, title: 'Mobile', code: 'TM', weight: 0.13, hub: 377 },
];

export function buildGraphMap({
  tasks = 200,
  pages = 50,
  links = 420,
  orphans = 5,
  seed = 7,
  now = Date.now(),
} = {}) {
  const rand = rng(seed);
  const pick = (list) => list[Math.floor(rand() * list.length)];
  const stamp = (ms) => new Date(ms).toISOString();
  const nodes = [];
  const byCluster = new Map();
  const add = (cluster, node) => {
    nodes.push(node);
    if (!byCluster.has(cluster)) byCluster.set(cluster, []);
    byCluster.get(cluster).push(node);
  };
  const makeDates = () => {
    const created = now - Math.floor(rand() * 95 * DAY);
    const updated = Math.min(
      now,
      created + Math.floor(rand() * (rand() < 0.6 ? 12 : 60) * DAY),
    );
    return { created_at: stamp(created), updated_at: stamp(updated) };
  };
  const statuses = [
    { name: 'Backlog', type: 'default' },
    { name: 'In progress', type: 'active' },
    { name: 'Review', type: 'active' },
    { name: 'Done', type: 'completed' },
  ];

  let nextKey = 300;
  const reserved = new Set([377, 385, 389, 399, 408, 416]);
  const nextNum = () => {
    while (reserved.has(nextKey)) nextKey++;
    return nextKey++;
  };
  for (const cat of MAP_CATEGORIES) {
    const n = Math.round((tasks * cat.weight) / 0.9);
    for (let i = 0; i < n; i++) {
      const num = i === 0 ? cat.hub : cat.id === 2 && i === 1 ? 416 : nextNum();
      const id = 10000 + nodes.length;
      add(`c${cat.id}`, {
        id: `task:${id}`,
        type: 'task',
        ref_id: id,
        key: `TM-${num}`,
        title: `TM-${num}: ${cat.title} ${i === 0 ? 'overview' : 'item ' + i}`,
        status: pick(statuses),
        category_id: cat.id,
        category: cat.title,
        hop: 0,
        weight: 1,
        rel: null,
        meta: { ...makeDates(), degree: 0, blocks: 0 },
      });
    }
  }
  for (let i = 0; i < pages; i++) {
    add('pages', {
      id: `page:${i + 1}`,
      type: 'page',
      ref_id: i + 1,
      key: null,
      title: i === 0 ? 'TM-388 Pages' : `Page ${i + 1}`,
      status: null,
      category_id: null,
      category: null,
      hop: 0,
      weight: 1,
      rel: null,
      meta: { ...makeDates(), degree: 0, blocks: 0, slug: `page-${i + 1}` },
    });
  }

  const orphanSet = new Set();
  const pool = nodes.filter((n) => n.key !== 'TM-416');
  while (orphanSet.size < orphans) {
    const n = pick(pool);
    if (!n.key || !/TM-(399|385|408|389|377)/.test(n.key)) orphanSet.add(n.id);
  }

  const edges = [];
  const seen = new Set();
  const connect = (a, b, type) => {
    if (a === b || orphanSet.has(a.id) || orphanSet.has(b.id)) return;
    const id = `${type}:${a.id}:${b.id}`;
    const rev = `${type}:${b.id}:${a.id}`;
    if (seen.has(id) || seen.has(rev)) return;
    seen.add(id);
    edges.push({
      id,
      from: a.id,
      to: b.id,
      type,
      label: type.replace('_', ' '),
      weight: type === 'blocks' ? 3 : 2,
      why: `${a.key ?? a.title} ${type.replace('_', ' ')} ${b.key ?? b.title}`,
    });
  };
  const typeFor = (a, b) =>
    a.type === 'page' && b.type === 'page'
      ? 'links_to'
      : a.type === 'page'
      ? 'linked_page'
      : b.type === 'page'
      ? 'mentioned_in'
      : pick(['relates_to', 'relates_to', 'depends_on']);

  const clusters = [...byCluster.values()];
  for (const members of clusters) {
    const hub = members[0];
    for (let i = 1; i < members.length; i++) {
      const m = members[i];
      if (rand() < 0.62) connect(hub, m, typeFor(hub, m));
      else connect(members[Math.floor(rand() * i)], m, typeFor(members[0], m));
    }
  }
  const all = nodes.filter((n) => !orphanSet.has(n.id));
  const cross = Math.round(links * 0.1);
  const agents = byCluster.get('c4');
  const crossPools = [
    byCluster.get('pages'),
    byCluster.get('c1'),
    byCluster.get('c2'),
  ];
  for (let i = 0; i < cross; i++) {
    const a = rand() < 0.55 ? pick(agents) : pick(all);
    const other = rand() < 0.6 ? pick(pick(crossPools)) : pick(all);
    connect(a, other, typeFor(a, other));
  }
  let guard = 0;
  while (edges.length < links && guard++ < links * 20) {
    const members = pick(clusters);
    const a = pick(members);
    const b = rand() < 0.35 ? members[0] : pick(members);
    connect(a, b, typeFor(a, b));
  }

  const bottleneck = nodes.find((n) => n.key === 'TM-416');
  const blocked = [
    ...byCluster.get('c2').slice(5, 7),
    ...byCluster.get('c4').slice(5, 7),
  ];
  for (const t of blocked) connect(bottleneck, t, 'blocks');
  const recent = {
    created_at: stamp(now - 45 * DAY),
    updated_at: stamp(now - DAY),
  };
  for (const n of [
    bottleneck,
    ...blocked,
    ...clusters.map((members) => members[0]),
  ]) {
    Object.assign(n.meta, recent);
  }

  const degree = new Map(nodes.map((n) => [n.id, 0]));
  const blocks = new Map(nodes.map((n) => [n.id, 0]));
  for (const e of edges) {
    degree.set(e.from, degree.get(e.from) + 1);
    degree.set(e.to, degree.get(e.to) + 1);
    if (e.type === 'blocks') blocks.set(e.from, blocks.get(e.from) + 1);
  }
  for (const n of nodes) {
    n.meta.degree = degree.get(n.id);
    n.meta.blocks = blocks.get(n.id);
    n.weight = n.meta.degree;
  }
  const rank = (n) => ({
    node: n,
    degree: n.meta.degree,
    blocks: n.meta.blocks,
  });
  const hubs = [...nodes]
    .sort((a, b) => b.meta.degree - a.meta.degree)
    .slice(0, 5)
    .map(rank);
  const bottlenecks = nodes
    .filter((n) => n.meta.blocks > 0)
    .sort((a, b) => b.meta.blocks - a.meta.blocks)
    .slice(0, 5)
    .map(rank);
  const catOf = new Map(nodes.map((n) => [n.id, n.category_id]));
  const pairs = new Map();
  for (const e of edges) {
    const a = catOf.get(e.from);
    const b = catOf.get(e.to);
    if (a === b) continue;
    const key = `${a}|${b}`;
    pairs.set(key, (pairs.get(key) ?? 0) + 1);
  }
  const bridges = [...pairs.entries()]
    .sort((x, y) => y[1] - x[1])
    .slice(0, 10)
    .map(([key, count]) => {
      const [from, to] = key.split('|');
      return {
        from_category_id: from === 'null' ? null : Number(from),
        to_category_id: to === 'null' ? null : Number(to),
        edges: count,
      };
    });
  const orphanNodes = nodes.filter((n) => n.meta.degree === 0);

  return {
    nodes,
    edges,
    categories: [
      ...MAP_CATEGORIES.map((c) => ({
        id: c.id,
        title: c.title,
        code: c.code,
        count: byCluster.get(`c${c.id}`).length,
      })),
      { id: null, title: 'Pages & docs', code: null, count: pages },
    ],
    insights: {
      hubs,
      bottlenecks,
      bridges,
      orphans: { total: orphanNodes.length, ids: orphanNodes.map((n) => n.id) },
    },
    truncated: false,
    total: nodes.length,
  };
}

export function filterByRange(map, from, to) {
  const fromMs = from ? new Date(`${from}T00:00:00`).getTime() : null;
  const toMs = new Date(`${to}T23:59:59.999`).getTime();
  const keep = new Set();
  const nodes = map.nodes.filter((n) => {
    const ok =
      Date.parse(n.meta.created_at) <= toMs &&
      (fromMs === null || Date.parse(n.meta.updated_at) >= fromMs);
    if (ok) keep.add(n.id);
    return ok;
  });
  return {
    ...map,
    nodes,
    edges: map.edges.filter((e) => keep.has(e.from) && keep.has(e.to)),
    total: nodes.length,
  };
}
