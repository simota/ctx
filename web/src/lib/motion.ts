// Reduced-motion helpers. The global CSS override in app.css cannot reach
// JS-driven `scrollIntoView({ behavior: 'smooth' })`, so callers route the
// behavior through `scrollBehavior()`.

export function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/** `wanted`, except that 'smooth' degrades to 'auto' under reduced motion. */
export function scrollBehavior(wanted: ScrollBehavior = 'smooth'): ScrollBehavior {
  return wanted === 'smooth' && prefersReducedMotion() ? 'auto' : wanted;
}
