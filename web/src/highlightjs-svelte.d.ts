// Type declaration for the community package 'highlightjs-svelte'.
// The package ships no bundled .d.ts; this shim provides the minimal typing
// needed for the registration call: `hljsSvelte(hljs)`.
declare module 'highlightjs-svelte' {
  import type { HLJSApi } from 'highlight.js';
  function hljsSvelte(hljs: HLJSApi): void;
  export default hljsSvelte;
}
