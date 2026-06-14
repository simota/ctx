// Commit-graph lane layout — the `git log --graph` swimlane algorithm.
//
// Given commits newest-first (each with its full hash and parent hashes),
// assign every commit a lane and emit, per row, the edges (rails) connecting
// the lanes between the row's top and bottom plus the commit's dot. Coordinates
// are lane-relative: `lane` is an integer column index; `y` is 0 (row top),
// 0.5 (dot center), or 1 (row bottom). The renderer maps lane→x and y→pixels.
//
// Lanes keep their column index across rows where possible (no crossing
// minimization), which keeps unrelated history as straight vertical rails and
// reads cleanly for the mostly-linear histories this tool targets.

export interface GraphEdge {
  fromLane: number;
  fromY: number;
  toLane: number;
  toY: number;
  color: number; // palette index
}

export interface GraphRow {
  dotLane: number;
  dotColor: number;
  /** Number of lane columns this row occupies (for sizing the gutter). */
  width: number;
  edges: GraphEdge[];
}

interface MinimalCommit {
  hash_full: string;
  parents?: string[];
}

function firstNull(lanes: (string | null)[]): number {
  const i = lanes.indexOf(null);
  return i;
}

/** Assign `hash` to a lane: reuse the first free slot, else append. */
function placeLane(lanes: (string | null)[], hash: string): number {
  const free = firstNull(lanes);
  if (free >= 0) {
    lanes[free] = hash;
    return free;
  }
  lanes.push(hash);
  return lanes.length - 1;
}

export function computeGraph(commits: MinimalCommit[]): GraphRow[] {
  // `lanes[i]` = the hash the i-th lane is currently waiting to reach.
  let lanes: (string | null)[] = [];
  const rows: GraphRow[] = [];

  for (const c of commits) {
    const h = c.hash_full;
    const parents = c.parents ?? [];
    const lanesIn = lanes.slice();

    let dotLane = lanesIn.indexOf(h);
    const isTip = dotLane === -1;
    if (isTip) {
      // A branch tip with no descendant in-window: start a fresh lane.
      dotLane = placeLane(lanes, h);
    }

    // Build the outgoing lane state: every lane awaiting this commit converges
    // into the dot (cleared), then parents take over lanes.
    const lanesOut = lanes.slice();
    for (let i = 0; i < lanesOut.length; i++) {
      if (lanesOut[i] === h) lanesOut[i] = null;
    }
    if (parents.length > 0) {
      lanesOut[dotLane] = parents[0];
      for (let k = 1; k < parents.length; k++) {
        // A parent already flowing in another lane reuses it; else a new lane.
        if (!lanesOut.includes(parents[k])) placeLane(lanesOut, parents[k]);
      }
    } else {
      lanesOut[dotLane] = null;
    }

    const edges: GraphEdge[] = [];
    // Incoming rails from the row above.
    for (let i = 0; i < lanesIn.length; i++) {
      const v = lanesIn[i];
      if (v === null) continue;
      if (v === h) {
        // Converge into the dot (skip the dot's own lane on a tip — no rail).
        if (!(isTip && i === dotLane)) {
          edges.push({ fromLane: i, fromY: 0, toLane: dotLane, toY: 0.5, color: i });
        }
      } else {
        // Pass-through: this lane keeps flowing; find where it continues.
        const k = lanesOut.indexOf(v);
        if (k >= 0) edges.push({ fromLane: i, fromY: 0, toLane: k, toY: 1, color: i });
      }
    }
    // Diverging rails from the dot down to each parent's continuing lane.
    for (const p of parents) {
      const k = lanesOut.indexOf(p);
      if (k >= 0) edges.push({ fromLane: dotLane, fromY: 0.5, toLane: k, toY: 1, color: k });
    }

    const width = Math.max(lanesIn.length, lanesOut.length, dotLane + 1);
    rows.push({ dotLane, dotColor: dotLane, width, edges });

    // Trim trailing nulls so lanes don't grow unbounded.
    let end = lanesOut.length;
    while (end > 0 && lanesOut[end - 1] === null) end--;
    lanes = lanesOut.slice(0, end);
  }

  return rows;
}
