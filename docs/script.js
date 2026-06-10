/**
 * ctx LP — script.js v2
 * Theme toggle + smooth scroll + progress bar + section reveal + analytics hooks
 * ES module, no build step required.
 */

// ── Theme ────────────────────────────────────────────────────────────────────

const STORAGE_KEY = 'lp-theme';
const ATTR = 'data-theme';

function resolveInitialTheme() {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === 'light' || stored === 'dark') return stored;
  return window.matchMedia('(prefers-color-scheme: light)').matches
    ? 'light'
    : 'dark';
}

function applyTheme(theme) {
  document.documentElement.setAttribute(ATTR, theme);
  localStorage.setItem(STORAGE_KEY, theme);

  // Update toggle button label & aria
  const btn = document.getElementById('theme-toggle');
  if (!btn) return;
  const isDark = theme === 'dark';
  btn.setAttribute('aria-label', isDark ? 'ライトモードに切り替え' : 'ダークモードに切り替え');
  btn.setAttribute('aria-pressed', String(!isDark));
  const icon = btn.querySelector('.theme-icon');
  if (icon) icon.textContent = isDark ? '☀' : '◐';
}

function initTheme() {
  applyTheme(resolveInitialTheme());
}

function toggleTheme() {
  const current = document.documentElement.getAttribute(ATTR) || 'dark';
  applyTheme(current === 'dark' ? 'light' : 'dark');
}

// ── Smooth scroll for hash anchors ───────────────────────────────────────────

function initSmoothScroll() {
  const prefersReducedMotion = () =>
    window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  document.addEventListener('click', (e) => {
    const anchor = e.target.closest('a[href^="#"]');
    if (!anchor) return;
    const id = anchor.getAttribute('href').slice(1);
    if (!id) return;
    const target = document.getElementById(id);
    if (!target) return;
    e.preventDefault();
    target.scrollIntoView({
      behavior: prefersReducedMotion() ? 'auto' : 'smooth',
      block: 'start',
    });
    target.focus({ preventScroll: true });
  });
}

// ── Hero terminal progress bar animation ─────────────────────────────────────

function initProgressBar() {
  const bar = document.querySelector('.term-progress-fill');
  if (!bar) return;

  // Set will-change before animation starts
  bar.style.willChange = 'width';

  // Trigger fill after a short paint delay
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      bar.style.width = bar.dataset.fill || '71%';
    });
  });

  // Flow Track E Note 3: release will-change after transition completes
  bar.addEventListener('transitionend', () => {
    bar.style.willChange = 'auto';
  }, { once: true });
}

// ── Section reveal via IntersectionObserver ───────────────────────────────────
// Flow Track E Note 2: IntersectionObserver-based section reveal

function initSectionReveal() {
  const sections = document.querySelectorAll('[data-event="section_entered"]');
  if (!sections.length || !('IntersectionObserver' in window)) {
    sections.forEach((s) => s.classList.add('is-visible'));
    return;
  }
  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          entry.target.classList.add('is-visible');
          // Optional: emit analytics event if PostHog/GA4 wired (Pulse Track C)
          // window.posthog?.capture('section_entered', { section: entry.target.dataset.section });
          observer.unobserve(entry.target);
        }
      });
    },
    { threshold: 0.1, rootMargin: '0px 0px -40px 0px' }
  );
  sections.forEach((s) => observer.observe(s));
}

// ── CTA analytics hooks ───────────────────────────────────────────────────────
// Funnel data-event: cta_primary_clicked / cta_secondary_clicked
// Attributes are present on elements; hooks are noop until PostHog/GA4 wired

function initCtaAnalytics() {
  const ctaPrimary = document.querySelectorAll('[data-event="cta_primary_clicked"]');
  ctaPrimary.forEach((el) => {
    el.addEventListener('click', () => {
      // window.posthog?.capture('cta_primary_clicked', { position: el.dataset.position });
    });
  });

  const ctaSecondary = document.querySelectorAll('[data-event="cta_secondary_clicked"]');
  ctaSecondary.forEach((el) => {
    el.addEventListener('click', () => {
      // window.posthog?.capture('cta_secondary_clicked', { position: el.dataset.position });
    });
  });
}

// ── FAQ analytics hooks ───────────────────────────────────────────────────────

function initFaqAnalytics() {
  const faqs = document.querySelectorAll('[data-event="faq_open"]');
  faqs.forEach((el) => {
    el.addEventListener('toggle', () => {
      if (el.open) {
        // window.posthog?.capture('faq_open', { faq_id: el.dataset.faqId });
      }
    });
  });
}

// ── A/B Experiment — Hero Variant (Track D) ───────────────────────────────────
// Experiment: HERO_VARIANT_001
// Variant A (control): caption + h1 + hero-sub-jp + hero-sub-en + CTA
// Variant B (Helix 潔さ型): caption + h1 + hero-sub-en only (remove hero-sub-jp)
// Assignment: deterministic hash → localStorage fallback → random
// Primary metric: cta_primary_clicked[data-position="hero"]
// Pre-registered hypothesis:
//   「英語 1 行のみにすることで Curator 純度が上がり、
//    Orion の Star 動機が強化され、hero CTR が ≥ 10% 相対増加する。
//    一方 Sora の friction は上昇し、hero 直下 scroll 率が低下する可能性がある。」

const AB_EXPERIMENT_ID = 'HERO_VARIANT_001';
const AB_STORAGE_KEY   = 'ab-' + AB_EXPERIMENT_ID;
const AB_VARIANTS      = ['A', 'B'];

/**
 * Deterministic bucket: djb2 hash of a stable seed string → variant index.
 * Stable across sessions for the same seed without cookies.
 * @param {string} seed
 * @returns {'A'|'B'}
 */
function hashVariant(seed) {
  let h = 5381;
  for (let i = 0; i < seed.length; i++) {
    h = ((h << 5) + h) ^ seed.charCodeAt(i);
    h = h >>> 0; // unsigned 32-bit
  }
  return AB_VARIANTS[h % AB_VARIANTS.length];
}

/**
 * Build a stable seed from non-personal browser signals.
 * Deliberately excludes IP, cookies, and PII — GDPR/個人情報保護法 minimal.
 */
function buildSeed() {
  const ua = navigator.userAgent || '';
  const tz = Intl.DateTimeFormat().resolvedOptions().timeZone || '';
  const vw = String(window.screen.width || 0);
  return ua + '|' + tz + '|' + vw;
}

/**
 * Assign (or recall) experiment variant.
 * Priority: localStorage → deterministic hash → random fallback.
 * @returns {'A'|'B'}
 */
function assignVariant() {
  try {
    const stored = localStorage.getItem(AB_STORAGE_KEY);
    if (stored === 'A' || stored === 'B') return stored;
  } catch (_) { /* private browsing: localStorage blocked */ }

  let variant;
  try {
    variant = hashVariant(buildSeed());
  } catch (_) {
    variant = Math.random() < 0.5 ? 'A' : 'B';
  }

  try {
    localStorage.setItem(AB_STORAGE_KEY, variant);
  } catch (_) { /* ignore */ }

  return variant;
}

/**
 * Apply Variant B (Helix 潔さ型):
 * Remove Japanese hero-sub text, retain English line only.
 * DOM manipulation targets `.hero-sub` — runs before first paint via DOMContentLoaded.
 */
function applyVariantB() {
  const heroSub = document.querySelector('.hero-sub');
  if (!heroSub) return;

  // Extract and preserve the English span
  const enSpan = heroSub.querySelector('.hero-sub-en');
  if (!enSpan) return;

  // Replace entire paragraph with English-only content
  heroSub.innerHTML = '';
  heroSub.appendChild(enSpan);
  heroSub.setAttribute('data-ab-variant', 'B');
}

/**
 * Initialize A/B experiment:
 * 1. Assign variant
 * 2. Apply DOM mutation for variant B
 * 3. Emit exposure event (PostHog / GA4 ready)
 * 4. Annotate primary CTA with variant for downstream metric slicing
 */
function initAbExperiment() {
  const variant = assignVariant();

  // Mark <html> for CSS targeting and analytics dimension
  document.documentElement.setAttribute('data-ab-variant', variant);

  if (variant === 'B') {
    applyVariantB();
  }

  // Exposure event — wire to PostHog / GA4 when ready
  // window.posthog?.capture('experiment_exposure', {
  //   experiment_id: AB_EXPERIMENT_ID,
  //   variant,
  //   timestamp: Date.now(),
  // });
  // window.gtag?.('event', 'experiment_exposure', {
  //   experiment_id: AB_EXPERIMENT_ID,
  //   variant,
  // });

  // Annotate all primary CTAs with variant so conversion events carry split context
  document.querySelectorAll('[data-event="cta_primary_clicked"]').forEach((el) => {
    el.setAttribute('data-ab-variant', variant);
  });
}

// ── Init ─────────────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
  initAbExperiment(); // Must run before initCtaAnalytics to annotate CTAs
  initTheme();
  initSmoothScroll();
  initProgressBar();
  initSectionReveal();
  initCtaAnalytics();
  initFaqAnalytics();

  const btn = document.getElementById('theme-toggle');
  if (btn) btn.addEventListener('click', toggleTheme);
});
