// Shared highlight.js instance with all extra language registrations.
//
// Import this module instead of 'highlight.js/lib/common' so that community
// language extensions are registered exactly once, regardless of how many
// components import hljs.

import hljs from 'highlight.js/lib/common';
import hljsKotlin from 'highlight.js/lib/languages/kotlin';
import hljsSwift from 'highlight.js/lib/languages/swift';
import hljsDockerfile from 'highlight.js/lib/languages/dockerfile';
import hljsSvelte from 'highlightjs-svelte';

hljs.registerLanguage('kotlin', hljsKotlin);
hljs.registerLanguage('swift', hljsSwift);
// `makefile` ships in highlight.js/lib/common; `dockerfile` does not, so
// register it here for the Dockerfile source view.
hljs.registerLanguage('dockerfile', hljsDockerfile);

// Register Svelte language (script/style blocks are sub-highlighted as
// JavaScript/CSS automatically by the definition).
hljsSvelte(hljs);

export default hljs;
