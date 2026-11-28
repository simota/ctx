import { test } from 'node:test';
import assert from 'node:assert/strict';
import { classifyFile } from './file-kind.ts';

test('markdown and mermaid files are previewable', () => {
  assert.equal(classifyFile('README.md').isMarkdown, true);
  assert.equal(classifyFile('docs/a.MDX').isPreviewable, true);
  assert.equal(classifyFile('flow.mmd').isMmd, true);
  assert.equal(classifyFile('src/main.rs').isPreviewable, false);
});

test('svg and html are not routed to the xml insights sidebar', () => {
  const svg = classifyFile('icons/logo.svg');
  assert.equal(svg.isSvg, true);
  assert.equal(svg.isXml, false);
  assert.equal(svg.isBinaryView, false, 'SVG keeps its script-safe data-URL preview');
  assert.equal(classifyFile('page.xhtml').isXml, false);
  assert.equal(classifyFile('feed.rss').isXml, true);
});

test('binary media render as image or pdf, never as source', () => {
  for (const p of ['a.png', 'b.JPEG', 'c.webp', 'd.ico']) {
    const k = classifyFile(p);
    assert.equal(k.isImage, true, p);
    assert.equal(k.isBinaryView, true, p);
  }
  assert.equal(classifyFile('spec.pdf').isBinaryView, true);
});

test('build files are recognised by basename and by suffix', () => {
  for (const p of ['Makefile', 'sub/GNUmakefile', 'rules.mk']) {
    assert.equal(classifyFile(p).configRenderKind, 'makefile', p);
  }
  for (const p of ['Dockerfile', 'ci/Dockerfile.dev', 'Containerfile', 'app.dockerfile']) {
    assert.equal(classifyFile(p).configRenderKind, 'dockerfile', p);
  }
  // A directory named like a build file does not make its children build files.
  assert.equal(classifyFile('Makefile/notes.txt').isMakefile, false);
  assert.equal(classifyFile('MAKEFILE').isMakefile, false, 'basename match is case-sensitive');
});

test('config files map to their structured view', () => {
  assert.equal(classifyFile('ci.yml').configRenderKind, 'yaml');
  assert.equal(classifyFile('package.json').configRenderKind, 'json');
  assert.equal(classifyFile('Cargo.toml').configRenderKind, 'toml');
  assert.equal(classifyFile('pom.xml').configRenderKind, 'xml');
  assert.equal(classifyFile('main.go').configRenderKind, '');
  assert.equal(classifyFile('main.go').isConfigRender, false);
});

test('relations and tests sidebars follow the supported language lists', () => {
  assert.equal(classifyFile('a.svelte').isRelationsTarget, true);
  assert.equal(classifyFile('a.rb').isRelationsTarget, false);
  assert.equal(classifyFile('a.rb').isTestsTarget, true);
  assert.equal(classifyFile('a.hpp').isTestsTarget, true);
  assert.equal(classifyFile('README.md').isTestsTarget, false);
});
