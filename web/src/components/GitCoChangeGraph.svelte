<script lang="ts">
  import {
    forceSimulation,
    forceManyBody,
    forceLink,
    forceCenter,
    forceCollide,
    forceX,
    forceY,
    type SimulationNodeDatum,
    type SimulationLinkDatum,
  } from 'd3-force';
  import { basename } from '../lib/format';
  import { route, navigate, toFileHash } from '../lib/router.svelte';
  import { cochange, loadCoChange } from '../lib/cochange.svelte';
  import type { CoChangeNode } from '../lib/api';

  // Palette shared with GitLogList — one color per directory group.
  const PALETTE = ['#4e9cf6', '#36b37e', '#f2b53d', '#e35d6a', '#9d7be8', '#46c4d4', '#e8884a', '#8bbf4a'];
  const TOP_K = 60; // cap rendered nodes; the rest collapse to "+N more".
  const LABEL_TOP = 12; // always-on labels for the N most-committed nodes.
  const W = 800; // viewBox width
  const H = 600; // viewBox height
  const PAD = 40; // fit-to-content padding (room for circles + halo'd labels)
  const TICKS = 320; // synchronous layout passes before freezing the SVG

  // A laid-out node: API fields + resolved screen coords / radius / color.
  interface LaidNode {
    path: string;
    commits: number;
    lines: number; // file size (LOC); 0 when unknown
    r: number; // radius ∝ lines
    heat: number; // churn intensity 0..1 (commits / maxCommits) → glow
    color: string;
    x: number;
    y: number;
    big: boolean; // top-LABEL_TOP by commits → label always shown
  }
  interface LaidEdge {
    source: number; // index into the rendered nodes array
    target: number;
    weight: number;
    width: number;
  }
  // d3-force mutates these in place; keep the original index for edge mapping.
  type SimNode = SimulationNodeDatum & { idx: number };
  type SimLink = SimulationLinkDatum<SimNode> & { weight: number };

  let loading = $derived(cochange.loading);
  let error = $derived(cochange.error);
  let data = $derived(cochange.data);

  // Drive the date filter off the active tree/file filter (route.since) so the
  // relations graph honors the same window as the rest of the app.
  $effect(() => {
    void loadCoChange(500, route.since ?? '', 2);
  });

  // Color-grouping key. Most files here live under crates/<name>/… so the bare
  // first segment ("crates") collapses everything to one color. Use the first
  // TWO segments (crate unit) when the path is nested, else the first segment.
  function groupKey(path: string): string {
    const segs = path.split('/');
    if (segs.length >= 3) return `${segs[0]}/${segs[1]}`;
    return segs[0];
  }

  const R_MIN = 6; // smallest readable radius (also the lines-unknown fallback)
  const R_MAX = 24; // largest radius for the biggest file in view

  // sqrt scale for node radius (area ∝ lines); clamp to a readable band.
  // Files with no/zero `lines` collapse to the minimum dot so they never vanish.
  function radiusFor(lines: number, maxLines: number): number {
    if (!Number.isFinite(lines) || lines <= 0 || maxLines <= 0) return R_MIN;
    return R_MIN + (R_MAX - R_MIN) * Math.sqrt(lines / maxLines);
  }

  let hovered = $state<number | null>(null);

  // Layout: select Top-K nodes, build a color map, run d3-force synchronously,
  // then clamp into the viewBox. Pure derivation of `data` — no reactivity
  // leaks into the simulation (it runs to completion here, then we read coords).
  interface Layout {
    nodes: LaidNode[];
    edges: LaidEdge[];
    total: number;
    hidden: number;
    dirColors: [string, string][];
  }

  let layout = $derived.by<Layout | null>(() => {
    if (!data || data.nodes.length === 0) return null;

    // Top-K by commit count; keep the original indices to remap edges.
    const ranked = data.nodes
      .map((n, i) => ({ n, i }))
      .sort((a, b) => b.n.commits - a.n.commits);
    const kept = ranked.slice(0, TOP_K);
    const total = data.nodes.length;
    const hidden = Math.max(0, total - kept.length);

    // old index -> new index (kept only)
    const remap = new Map<number, number>();
    kept.forEach((e, newIdx) => remap.set(e.i, newIdx));

    const keptNodes: CoChangeNode[] = kept.map((e) => e.n);
    // commits drive glow intensity; lines drive radius. Track both maxes.
    const maxCommits = keptNodes.reduce((m, n) => Math.max(m, n.commits), 0);
    const maxLines = keptNodes.reduce((m, n) => Math.max(m, n.lines ?? 0), 0);

    // Stable per-group color assignment (first-seen order).
    const dirColor = new Map<string, string>();
    for (const n of keptNodes) {
      const d = groupKey(n.path);
      if (!dirColor.has(d)) dirColor.set(d, PALETTE[dirColor.size % PALETTE.length]);
    }

    // commit threshold for "always-on label" — top LABEL_TOP nodes.
    const labelCut =
      keptNodes.length === 0
        ? Infinity
        : [...keptNodes]
            .map((n) => n.commits)
            .sort((a, b) => b - a)[Math.min(LABEL_TOP, keptNodes.length) - 1];

    // Only edges whose BOTH endpoints survived the Top-K cut.
    const edges = data.edges
      .filter((e) => remap.has(e.source) && remap.has(e.target))
      .map((e) => ({
        source: remap.get(e.source) as number,
        target: remap.get(e.target) as number,
        weight: e.weight,
      }));
    const maxWeight = edges.reduce((m, e) => Math.max(m, e.weight), 1);

    // d3-force simulation, run to completion synchronously. Weak forceX/forceY
    // pull disconnected components toward center so they don't drift apart;
    // charge/link/collide give connected clusters breathing room.
    const sim = forceSimulation<SimNode>(
      keptNodes.map((_, idx) => ({ idx })),
    )
      .force(
        'link',
        forceLink<SimNode, SimLink>(
          edges.map((e) => ({ source: e.source, target: e.target, weight: e.weight })),
        )
          .id((d) => d.idx)
          .distance((l) => 36 + 54 / (1 + (l.weight ?? 1)))
          .strength((l) => 0.25 + 0.5 * ((l.weight ?? 1) / maxWeight)),
      )
      .force('charge', forceManyBody().strength(-260).distanceMax(360))
      .force('center', forceCenter(W / 2, H / 2))
      .force('x', forceX(W / 2).strength(0.045))
      .force('y', forceY(H / 2).strength(0.06))
      .force(
        'collide',
        forceCollide<SimNode>()
          .radius((d) => radiusFor(keptNodes[d.idx].lines ?? 0, maxLines) + 7)
          .strength(0.85),
      )
      .stop();
    sim.tick(TICKS);

    const simNodes = sim.nodes();
    const placed = keptNodes.map((n, idx) => {
      const r = radiusFor(n.lines ?? 0, maxLines);
      return {
        n,
        r,
        sx: simNodes[idx].x ?? W / 2,
        sy: simNodes[idx].y ?? H / 2,
      };
    });

    // Fit-to-content: uniform scale + translate so the laid-out cloud fills the
    // viewBox with PAD margin (replaces per-node hard clamping). The bbox
    // accounts for each node's radius so circles never spill past the edge.
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const p of placed) {
      minX = Math.min(minX, p.sx - p.r);
      minY = Math.min(minY, p.sy - p.r);
      maxX = Math.max(maxX, p.sx + p.r);
      maxY = Math.max(maxY, p.sy + p.r);
    }
    const spanX = Math.max(1, maxX - minX);
    const spanY = Math.max(1, maxY - minY);
    const scale = Math.min((W - 2 * PAD) / spanX, (H - 2 * PAD) / spanY, 1.6);
    const offX = (W - spanX * scale) / 2 - minX * scale;
    const offY = (H - spanY * scale) / 2 - minY * scale;

    const laidNodes: LaidNode[] = placed.map((p) => ({
      path: p.n.path,
      commits: p.n.commits,
      lines: p.n.lines ?? 0,
      r: p.r,
      // Churn → glow. Normalize against the busiest file; ease so only genuine
      // hotspots light up strongly instead of a uniform wash.
      heat: maxCommits > 0 ? Math.pow(p.n.commits / maxCommits, 1.5) : 0,
      color: dirColor.get(groupKey(p.n.path)) ?? PALETTE[0],
      x: p.sx * scale + offX,
      y: p.sy * scale + offY,
      big: p.n.commits >= labelCut,
    }));

    const laidEdges: LaidEdge[] = edges.map((e) => ({
      ...e,
      width: 0.6 + 3 * (e.weight / maxWeight),
    }));

    return {
      nodes: laidNodes,
      edges: laidEdges,
      total,
      hidden,
      dirColors: [...dirColor.entries()],
    };
  });

  // An edge is "active" when it touches the hovered node.
  function edgeActive(e: LaidEdge): boolean {
    return hovered === null || e.source === hovered || e.target === hovered;
  }
  // A node is "active" when hovered or directly connected to the hovered one.
  function nodeActive(idx: number): boolean {
    if (hovered === null) return true;
    if (idx === hovered) return true;
    return (layout?.edges ?? []).some(
      (e) =>
        (e.source === hovered && e.target === idx) ||
        (e.target === hovered && e.source === idx),
    );
  }

  // Labels shown only for big (top-commit) nodes, the hovered node, and its
  // direct neighbors — keeps the canvas legible instead of a wall of text.
  function labelVisible(idx: number): boolean {
    if (layout?.nodes[idx]?.big) return true;
    if (hovered === null) return false;
    return nodeActive(idx);
  }

  function open(path: string): void {
    navigate(toFileHash(path));
  }
</script>

<section class="cochange" aria-label="file relationship graph">
  {#if loading}
    <p class="muted note" aria-busy="true">Loading relations…</p>
  {:else if error}
    <p class="note err"><code class="mono">{error}</code></p>
  {:else if !layout}
    <p class="muted note">No co-change relationships found.</p>
  {:else}
    <svg
      class="graph"
      viewBox="0 0 {W} {H}"
      role="img"
      aria-label="Co-change network of {layout.nodes.length} files"
      onmouseleave={() => (hovered = null)}
    >
      <defs>
        <radialGradient id="cc-sheen" cx="38%" cy="34%" r="72%">
          <stop offset="0%" stop-color="#fff" stop-opacity="0.45" />
          <stop offset="42%" stop-color="#fff" stop-opacity="0.08" />
          <stop offset="100%" stop-color="#000" stop-opacity="0.28" />
        </radialGradient>
        <filter id="cc-glow" x="-60%" y="-60%" width="220%" height="220%">
          <feGaussianBlur stdDeviation="3.2" result="b" />
          <feMerge>
            <feMergeNode in="b" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
        <!-- Soft color-preserving blur for the churn halo (hotspot glow). -->
        <filter id="cc-halo" x="-120%" y="-120%" width="340%" height="340%">
          <feGaussianBlur stdDeviation="5" />
        </filter>
      </defs>
      <g class="edges" aria-hidden="true">
        {#each layout.edges as e (`${e.source}-${e.target}`)}
          <line
            x1={layout.nodes[e.source].x}
            y1={layout.nodes[e.source].y}
            x2={layout.nodes[e.target].x}
            y2={layout.nodes[e.target].y}
            stroke={edgeActive(e) && hovered !== null
              ? 'var(--ctx-accent)'
              : 'var(--ctx-border-strong)'}
            stroke-width={e.width}
            stroke-linecap="round"
            opacity={edgeActive(e) ? (hovered !== null ? 0.55 : 0.32) : 0.06}
          />
        {/each}
      </g>
      <!-- Churn glow: hotspot halo behind the nodes. Radius + opacity scale
           with commit frequency so heavily-churned files visibly burn brighter.
           Skipped entirely when heat is ~0 to keep cold files crisp. -->
      <g class="glows" aria-hidden="true">
        {#each layout.nodes as n, i (n.path)}
          {#if n.heat > 0.04}
            <circle
              class="glow"
              cx={n.x}
              cy={n.y}
              r={n.r + 4 + 14 * n.heat}
              fill={n.color}
              opacity={(nodeActive(i) ? 1 : 0.16) * (0.18 + 0.46 * n.heat)}
              filter="url(#cc-halo)"
            />
          {/if}
        {/each}
      </g>
      <g class="nodes">
        {#each layout.nodes as n, i (n.path)}
          {@const active = nodeActive(i)}
          {@const hot = hovered === i}
          <g
            class="node"
            class:hot
            opacity={active ? 1 : 0.16}
            role="button"
            tabindex="0"
            aria-label="{n.path} ({n.lines > 0 ? `${n.lines} lines, ` : ''}{n.commits} commits) — double-click to open"
            onmouseenter={() => (hovered = i)}
            onfocus={() => (hovered = i)}
            onblur={() => (hovered = null)}
            ondblclick={() => open(n.path)}
            onkeydown={(ev) => {
              if (ev.key === 'Enter' || ev.key === ' ') {
                ev.preventDefault();
                open(n.path);
              }
            }}
          >
            <title>{n.path}{n.lines > 0 ? ` · ${n.lines} lines` : ''} · {n.commits} commits</title>
            {#if hot}
              <circle class="ring" cx={n.x} cy={n.y} r={n.r + 4} />
            {/if}
            <circle
              cx={n.x}
              cy={n.y}
              r={n.r}
              fill={n.color}
              stroke="var(--ctx-bg-panel, var(--ctx-bg))"
              stroke-width="1.5"
              filter={hot ? 'url(#cc-glow)' : undefined}
            />
            <circle
              class="sheen"
              cx={n.x}
              cy={n.y}
              r={n.r}
              fill="url(#cc-sheen)"
              aria-hidden="true"
            />
            {#if labelVisible(i)}
              <text
                x={n.x}
                y={n.y + n.r + 11}
                text-anchor="middle"
                class="label"
                class:strong={n.big || hot}
              >{basename(n.path)}</text>
            {/if}
          </g>
        {/each}
      </g>
    </svg>

    <footer class="legend">
      <span class="showing">
        Showing top {layout.nodes.length} of {layout.total} files
        {#if layout.hidden > 0}<span class="muted">(+{layout.hidden} more)</span>{/if}
      </span>
      <span class="legend-items muted">
        <span class="li">size = lines</span>
        <span class="sep" aria-hidden="true">·</span>
        <span class="li">glow = change frequency</span>
        <span class="sep" aria-hidden="true">·</span>
        <span class="li">line width = co-changes</span>
        <span class="sep" aria-hidden="true">·</span>
        <span class="li">color = module</span>
      </span>
      {#if layout.dirColors.length > 0}
        <span class="dirs">
          {#each layout.dirColors as [dir, color] (dir)}
            <span class="dir">
              <span class="swatch" style="background:{color}" aria-hidden="true"></span>{dir}
            </span>
          {/each}
        </span>
      {/if}
    </footer>
  {/if}
</section>

<style>
  .cochange {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .graph {
    flex: 1 1 auto;
    width: 100%;
    min-height: 0;
    display: block;
    background:
      radial-gradient(
        120% 120% at 50% 0%,
        color-mix(in srgb, var(--ctx-accent) 6%, transparent),
        transparent 60%
      );
  }
  .glow {
    pointer-events: none;
    /* Screen reads as additive light on dark themes; plus-lighter keeps it
       from washing out on light themes while still tinting the canvas. */
    mix-blend-mode: plus-lighter;
    transition: opacity 0.14s ease;
  }
  .node {
    cursor: pointer;
    transition: opacity 0.14s ease;
  }
  /* Subtle lift so equal-color nodes still read as discrete objects. */
  .node > circle:not(.sheen):not(.ring) {
    filter: drop-shadow(0 1px 1.5px rgba(0, 0, 0, 0.35));
  }
  .node .sheen {
    pointer-events: none;
    mix-blend-mode: soft-light;
  }
  .node .ring {
    fill: none;
    stroke: var(--ctx-accent);
    stroke-width: 1.5;
    opacity: 0.7;
    pointer-events: none;
  }
  .node:focus-visible circle:not(.sheen):not(.ring) {
    stroke: var(--ctx-accent);
    stroke-width: 2.5;
  }
  .label {
    font-size: 10px;
    fill: var(--ctx-fg-dim);
    pointer-events: none;
    font-family: var(--ctx-font-mono, var(--ctx-mono, ui-monospace, monospace));
    /* Halo: outline the glyphs so labels stay readable when they overlap. */
    paint-order: stroke;
    stroke: var(--ctx-bg-panel, var(--ctx-bg));
    stroke-width: 3px;
    stroke-linejoin: round;
  }
  .label.strong {
    fill: var(--ctx-fg);
  }
  .legend {
    flex: 0 0 auto;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 8px 12px;
    border-top: 1px solid var(--ctx-border);
    font-size: 0.76em;
    background: var(--ctx-bg-panel, var(--ctx-bg));
  }
  .showing {
    color: var(--ctx-fg);
  }
  .muted {
    color: var(--ctx-fg-dim);
  }
  .legend-items {
    display: inline-flex;
    gap: 5px;
    align-items: baseline;
  }
  .dirs {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    margin-left: auto;
  }
  .dir {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--ctx-fg-dim);
  }
  .swatch {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    display: inline-block;
  }
  .note {
    padding: 8px 12px;
    font-size: 0.85em;
  }
  .err {
    color: var(--ctx-err);
  }
  .mono {
    font-family: var(--ctx-font-mono, var(--ctx-mono, ui-monospace, monospace));
  }
</style>
