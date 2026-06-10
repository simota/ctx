// File-type icon resolver backed by VSCode's official icon set (vscode-icons,
// MIT). Icons are compiled to Svelte components by unplugin-icons at build
// time; we register only the ones we actually use so the bundle stays small
// (each icon adds ~1-2 KB after gzip).
//
// To add a new icon: import it from `~icons/vscode-icons/<icon-name>` and add
// an entry to BASENAME_ICON or EXT_ICON. Available icon names live at
// https://github.com/vscode-icons/vscode-icons/tree/master/icons.

import type { Component } from 'svelte';

import IconDefault from '~icons/vscode-icons/default-file';
import IconBinary from '~icons/vscode-icons/file-type-binary';
import IconText from '~icons/vscode-icons/file-type-text';

// Languages
import IconTypeScript from '~icons/vscode-icons/file-type-typescript';
import IconTypeScriptDef from '~icons/vscode-icons/file-type-typescriptdef';
import IconJS from '~icons/vscode-icons/file-type-js';
import IconReactTs from '~icons/vscode-icons/file-type-reactts';
import IconReactJs from '~icons/vscode-icons/file-type-reactjs';
import IconGo from '~icons/vscode-icons/file-type-go-gopher';
import IconGoPackage from '~icons/vscode-icons/file-type-go-package';
import IconPython from '~icons/vscode-icons/file-type-python';
import IconRust from '~icons/vscode-icons/file-type-rust';
import IconJava from '~icons/vscode-icons/file-type-java';
import IconKotlin from '~icons/vscode-icons/file-type-kotlin';
import IconSwift from '~icons/vscode-icons/file-type-swift';
import IconPHP from '~icons/vscode-icons/file-type-php';
import IconC from '~icons/vscode-icons/file-type-c';
import IconCpp from '~icons/vscode-icons/file-type-cpp';
import IconCSharp from '~icons/vscode-icons/file-type-csharp';
import IconRuby from '~icons/vscode-icons/file-type-ruby';
import IconShell from '~icons/vscode-icons/file-type-shell';

// Web / markup
import IconHTML from '~icons/vscode-icons/file-type-html';
import IconCSS from '~icons/vscode-icons/file-type-css';
import IconSCSS from '~icons/vscode-icons/file-type-scss';
import IconSass from '~icons/vscode-icons/file-type-sass';
import IconLess from '~icons/vscode-icons/file-type-less';
import IconSvelte from '~icons/vscode-icons/file-type-svelte';
import IconVue from '~icons/vscode-icons/file-type-vue';

// Data / config
import IconJSON from '~icons/vscode-icons/file-type-json';
import IconYAML from '~icons/vscode-icons/file-type-yaml';
import IconTOML from '~icons/vscode-icons/file-type-toml';
import IconXML from '~icons/vscode-icons/file-type-xml';
import IconSQL from '~icons/vscode-icons/file-type-sql';
import IconDotenv from '~icons/vscode-icons/file-type-dotenv';

// Docs / media
import IconMarkdown from '~icons/vscode-icons/file-type-markdown';
import IconImage from '~icons/vscode-icons/file-type-image';
import IconPdf from '~icons/vscode-icons/file-type-pdf2';

// Misc tooling
import IconZip from '~icons/vscode-icons/file-type-zip';
import IconDocker from '~icons/vscode-icons/file-type-docker';
import IconNpm from '~icons/vscode-icons/file-type-npm';
import IconLicense from '~icons/vscode-icons/file-type-license';
import IconGit from '~icons/vscode-icons/file-type-git';

// Match by exact basename first (case-insensitive). Keep this list short — when
// a file is best identified by its name (Dockerfile, package.json), the icon
// should reflect that, not the extension.
const BASENAME_ICON: Record<string, Component> = {
  dockerfile: IconDocker,
  'docker-compose.yml': IconDocker,
  'docker-compose.yaml': IconDocker,
  'package.json': IconNpm,
  'package-lock.json': IconNpm,
  'pnpm-lock.yaml': IconNpm,
  'yarn.lock': IconNpm,
  'go.mod': IconGoPackage,
  'go.sum': IconGoPackage,
  'license': IconLicense,
  'license.md': IconLicense,
  'license.txt': IconLicense,
  '.gitignore': IconGit,
  '.gitattributes': IconGit,
  '.gitmodules': IconGit,
  '.env': IconDotenv,
  '.env.local': IconDotenv,
  '.env.example': IconDotenv,
  '.env.development': IconDotenv,
  '.env.production': IconDotenv,
};

const EXT_ICON: Record<string, Component> = {
  // TypeScript / JavaScript
  ts: IconTypeScript,
  mts: IconTypeScript,
  cts: IconTypeScript,
  tsx: IconReactTs,
  js: IconJS,
  mjs: IconJS,
  cjs: IconJS,
  jsx: IconReactJs,
  // Go
  go: IconGo,
  // Python
  py: IconPython,
  pyi: IconPython,
  pyw: IconPython,
  // Rust
  rs: IconRust,
  // JVM
  java: IconJava,
  kt: IconKotlin,
  kts: IconKotlin,
  // Apple
  swift: IconSwift,
  // PHP
  php: IconPHP,
  // C / C++ / C#
  c: IconC,
  h: IconC,
  cpp: IconCpp,
  cc: IconCpp,
  cxx: IconCpp,
  hpp: IconCpp,
  hh: IconCpp,
  cs: IconCSharp,
  // Ruby
  rb: IconRuby,
  // Shell
  sh: IconShell,
  bash: IconShell,
  zsh: IconShell,
  fish: IconShell,
  // Web
  html: IconHTML,
  htm: IconHTML,
  css: IconCSS,
  scss: IconSCSS,
  sass: IconSass,
  less: IconLess,
  svelte: IconSvelte,
  vue: IconVue,
  // Data
  json: IconJSON,
  json5: IconJSON,
  jsonc: IconJSON,
  yaml: IconYAML,
  yml: IconYAML,
  toml: IconTOML,
  xml: IconXML,
  sql: IconSQL,
  // Docs
  md: IconMarkdown,
  markdown: IconMarkdown,
  mdx: IconMarkdown,
  txt: IconText,
  // Media
  png: IconImage,
  jpg: IconImage,
  jpeg: IconImage,
  gif: IconImage,
  webp: IconImage,
  svg: IconImage,
  ico: IconImage,
  bmp: IconImage,
  pdf: IconPdf,
  // Archives
  zip: IconZip,
  tar: IconZip,
  gz: IconZip,
  tgz: IconZip,
  '7z': IconZip,
  // Binary fallbacks
  exe: IconBinary,
  bin: IconBinary,
  dll: IconBinary,
  so: IconBinary,
  dylib: IconBinary,
  a: IconBinary,
};

// pickFileIcon returns the Svelte component to render for a given file path.
// Resolution order: exact basename → composite suffix (.d.ts) → extension →
// IconDefault. Always returns a component so callers can render unconditionally.
export function pickFileIcon(path: string | undefined | null): Component {
  if (!path) return IconDefault;
  const slash = path.lastIndexOf('/');
  const name = (slash === -1 ? path : path.slice(slash + 1)).toLowerCase();
  if (name in BASENAME_ICON) return BASENAME_ICON[name];
  if (name.endsWith('.d.ts')) return IconTypeScriptDef;
  const dot = name.lastIndexOf('.');
  if (dot === -1 || dot === name.length - 1) return IconDefault;
  return EXT_ICON[name.slice(dot + 1)] ?? IconDefault;
}
