import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  decodeHtmlEntities,
  escapeAttr,
  extractMermaidBlocks,
  makeSlugger,
  stripExecutableHtml,
} from './markdown-html.ts';

test('slugs keep unicode letters and disambiguate duplicates', () => {
  const slug = makeSlugger();
  assert.equal(slug('Getting Started!'), 'getting-started');
  assert.equal(slug('Getting Started'), 'getting-started-1');
  assert.equal(slug('インストール 手順'), 'インストール-手順');
  assert.equal(slug('  a -- b  '), 'a-b');
  // A fresh slugger starts counting again.
  assert.equal(makeSlugger()('Getting Started'), 'getting-started');
});

test('script blocks and inline handlers are stripped', () => {
  const html =
    '<p onclick="x()">a</p><script>alert(1)</script><img src=x onerror=alert(1)>' +
    "<div onmouseover='y()'>b</div><SCRIPT src=\"e.js\"/>";
  const out = stripExecutableHtml(html);
  assert.doesNotMatch(out, /<script/i);
  assert.doesNotMatch(out, /\son[a-z]+\s*=/i);
  assert.match(out, /<p>a<\/p>/);
  assert.match(out, /<div>b<\/div>/);
  assert.match(out, /<img src=x>/);
});

test('text that merely mentions handlers is left alone', () => {
  const html = '<p>Set the onclick attribute in your template.</p>';
  assert.equal(stripExecutableHtml(html), html);
});

test('mermaid blocks become placeholders carrying decoded source', () => {
  const html =
    '<p>x</p><pre><code class="hljs language-mermaid">graph TD; A--&gt;B &amp; C</code></pre>' +
    '<pre><code class="hljs language-rust">fn main() {}</code></pre>';
  const { html: out, blocks } = extractMermaidBlocks(html);
  assert.equal(blocks.length, 1);
  assert.equal(blocks[0].code, 'graph TD; A-->B & C');
  assert.ok(out.includes(`<div class="mermaid-block" data-mid="${blocks[0].id}"></div>`));
  assert.ok(out.includes('language-rust'), 'non-mermaid code blocks are untouched');
  const again = extractMermaidBlocks(html).blocks[0].id;
  assert.notEqual(again, blocks[0].id, 'placeholder ids are unique across renders');
});

test('entity decoding handles &amp; last so escapes are not double-decoded', () => {
  assert.equal(decodeHtmlEntities('&amp;lt;'), '&lt;');
  assert.equal(decodeHtmlEntities('&quot;a&#39;'), `"a'`);
});

test('attribute escaping covers quotes and ampersands', () => {
  assert.equal(escapeAttr('/raw/a&b/"c"/'), '/raw/a&amp;b/&quot;c&quot;/');
});
