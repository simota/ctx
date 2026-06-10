// SVG pan / zoom controller.
//
// Lightweight enough to avoid pulling in svg-pan-zoom. Mutates the SVG's
// viewBox attribute in response to wheel and pointer-drag, exposes a
// controller for programmatic zoom in/out/reset.
//
// Works on SVGs hosted inside a same-origin sandboxed iframe because the
// listeners are attached to the SVG element directly and pointer events
// fire on that element regardless of which window's code registered them.
//
// Wheel behaviour is gated by `wheelRequiresModifier` so the same helper
// can drive a standalone .mmd file (plain wheel = zoom) and inline mermaid
// blocks in a markdown document (Ctrl/Meta+wheel = zoom; bare wheel keeps
// page scrolling so the diagram doesn't hijack the user's reading flow).

export interface PanZoomController {
  zoomIn(): void;
  zoomOut(): void;
  reset(): void;
  destroy(): void;
}

export interface PanZoomOptions {
  /** Smallest allowed zoom — default 0.2 (the diagram shrinks to 1/5). */
  minZoom?: number;
  /** Largest allowed zoom — default 8 (8x the original viewBox). */
  maxZoom?: number;
  /** Wheel step factor — default 1.2 per notch. */
  zoomStep?: number;
  /** Programmatic zoom step (buttons) — default matches zoomStep. */
  buttonStep?: number;
  /** Require Ctrl/Meta to be held for wheel zoom — default false. */
  wheelRequiresModifier?: boolean;
}

export function attachPanZoom(
  svg: SVGSVGElement,
  opts: PanZoomOptions = {},
): PanZoomController {
  const minZoom = opts.minZoom ?? 0.2;
  const maxZoom = opts.maxZoom ?? 8;
  const wheelStep = opts.zoomStep ?? 1.2;
  const buttonStep = opts.buttonStep ?? opts.zoomStep ?? 1.4;
  const wheelRequiresModifier = !!opts.wheelRequiresModifier;

  // Seed the working viewBox from the SVG's own attribute; fall back to
  // getBBox() (the content's natural bounds) when the SVG was authored
  // without one — mermaid always emits viewBox, so the fallback is mostly
  // defensive for the future.
  const initial = readViewBox(svg);
  let [bx, by, bw, bh] = initial;
  if (!bw || !bh) {
    const bb = svg.getBBox();
    bx = bb.x;
    by = bb.y;
    bw = bb.width;
    bh = bb.height;
    svg.setAttribute('viewBox', `${bx} ${by} ${bw} ${bh}`);
  }
  let cx = bx;
  let cy = by;
  let cw = bw;
  let ch = bh;

  function apply() {
    svg.setAttribute('viewBox', `${cx} ${cy} ${cw} ${ch}`);
  }

  function screenToSvg(clientX: number, clientY: number): [number, number] {
    const rect = svg.getBoundingClientRect();
    const fx = rect.width > 0 ? (clientX - rect.left) / rect.width : 0.5;
    const fy = rect.height > 0 ? (clientY - rect.top) / rect.height : 0.5;
    return [cx + fx * cw, cy + fy * ch];
  }

  function zoomAt(clientX: number, clientY: number, factor: number) {
    // Constrain zoom so the new width/height stay within the
    // min/max-zoom-relative bounds. We compute against w0/h0 (initial),
    // not current, so the limits don't drift as the user pans.
    const minW = bw / maxZoom;
    const maxW = bw / minZoom;
    const newW = clamp(cw / factor, minW, maxW);
    const minH = bh / maxZoom;
    const maxH = bh / minZoom;
    const newH = clamp(ch / factor, minH, maxH);
    // Anchor the zoom at the cursor: the svg point under the cursor must
    // remain under the cursor after the zoom.
    const [sx, sy] = screenToSvg(clientX, clientY);
    const realFactorX = cw / newW;
    const realFactorY = ch / newH;
    cx = sx - (sx - cx) / realFactorX;
    cy = sy - (sy - cy) / realFactorY;
    cw = newW;
    ch = newH;
    apply();
  }

  function onWheel(e: WheelEvent) {
    if (wheelRequiresModifier && !(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    const factor = e.deltaY < 0 ? wheelStep : 1 / wheelStep;
    zoomAt(e.clientX, e.clientY, factor);
  }

  let dragging = false;
  let lastX = 0;
  let lastY = 0;

  function onPointerDown(e: PointerEvent) {
    // Only left-button / primary touch starts a pan. Buttons inside an
    // overlay would otherwise capture pointer here and start a drag.
    if (e.button !== 0 && e.pointerType === 'mouse') return;
    dragging = true;
    lastX = e.clientX;
    lastY = e.clientY;
    try {
      svg.setPointerCapture(e.pointerId);
    } catch {
      /* setPointerCapture can throw if the pointer is already captured */
    }
    svg.style.cursor = 'grabbing';
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const rect = svg.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;
    // Convert screen-pixel drag delta into viewBox units so panning feels
    // 1:1 with the cursor regardless of current zoom.
    const dx = (e.clientX - lastX) * (cw / rect.width);
    const dy = (e.clientY - lastY) * (ch / rect.height);
    cx -= dx;
    cy -= dy;
    lastX = e.clientX;
    lastY = e.clientY;
    apply();
  }

  function endDrag(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    try {
      svg.releasePointerCapture(e.pointerId);
    } catch {
      /* releasePointerCapture can throw when capture was lost */
    }
    svg.style.cursor = 'grab';
  }

  function onDblClick() {
    reset();
  }

  function reset() {
    cx = bx;
    cy = by;
    cw = bw;
    ch = bh;
    apply();
  }

  function zoomIn() {
    const rect = svg.getBoundingClientRect();
    zoomAt(rect.left + rect.width / 2, rect.top + rect.height / 2, buttonStep);
  }

  function zoomOut() {
    const rect = svg.getBoundingClientRect();
    zoomAt(rect.left + rect.width / 2, rect.top + rect.height / 2, 1 / buttonStep);
  }

  // SVG cursor + touch-action setup. touch-action: none lets pointer events
  // fire for two-finger gestures without the browser scrolling/zooming the
  // page underneath.
  const prevCursor = svg.style.cursor;
  const prevTouch = svg.style.touchAction;
  svg.style.cursor = 'grab';
  svg.style.touchAction = 'none';

  svg.addEventListener('wheel', onWheel, { passive: false });
  svg.addEventListener('pointerdown', onPointerDown);
  svg.addEventListener('pointermove', onPointerMove);
  svg.addEventListener('pointerup', endDrag);
  svg.addEventListener('pointercancel', endDrag);
  svg.addEventListener('dblclick', onDblClick);

  function destroy() {
    svg.removeEventListener('wheel', onWheel);
    svg.removeEventListener('pointerdown', onPointerDown);
    svg.removeEventListener('pointermove', onPointerMove);
    svg.removeEventListener('pointerup', endDrag);
    svg.removeEventListener('pointercancel', endDrag);
    svg.removeEventListener('dblclick', onDblClick);
    svg.style.cursor = prevCursor;
    svg.style.touchAction = prevTouch;
    // Restore the user-authored viewBox so a re-attach starts clean.
    svg.setAttribute('viewBox', `${bx} ${by} ${bw} ${bh}`);
  }

  return { zoomIn, zoomOut, reset, destroy };
}

function readViewBox(svg: SVGSVGElement): [number, number, number, number] {
  const raw = svg.getAttribute('viewBox');
  if (!raw) return [0, 0, 0, 0];
  const parts = raw.trim().split(/[\s,]+/).map(Number);
  if (parts.length !== 4 || parts.some((n) => Number.isNaN(n))) {
    return [0, 0, 0, 0];
  }
  return [parts[0], parts[1], parts[2], parts[3]];
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}
