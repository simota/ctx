// Shared highlight.js instance with all extra language registrations.
//
// Import this module instead of 'highlight.js/lib/common' so that community
// language extensions are registered exactly once, regardless of how many
// components import hljs.

import hljs from 'highlight.js/lib/common';
import hljsSvelte from 'highlightjs-svelte';

// Register Svelte language (script/style blocks are sub-highlighted as
// JavaScript/CSS automatically by the definition).
hljsSvelte(hljs);

export default hljs;
