// File-kind classification for the file viewer: which preview, insights
// sidebar and side panels a path gets. Pure (path in, flags out) so the
// rules can be unit-tested without mounting FileDetail.

export type ConfigRenderKind = '' | 'makefile' | 'dockerfile' | 'yaml' | 'json' | 'toml' | 'xml';

export interface FileKind {
  isSvg: boolean;
  isHtml: boolean;
  /** Binary images served straight from /raw (SVG excluded: it has its own script-safe path). */
  isImage: boolean;
  isPdf: boolean;
  /** isImage || isPdf — rendered as <img>/<iframe>, never as a source pane. */
  isBinaryView: boolean;
  isMarkdown: boolean;
  isCss: boolean;
  isJson: boolean;
  isXml: boolean;
  isYaml: boolean;
  isSql: boolean;
  isMakefile: boolean;
  isDockerfile: boolean;
  isToml: boolean;
  isMmd: boolean;
  /** Which structured "Rendered" view a config file gets ('' = none). */
  configRenderKind: ConfigRenderKind;
  isConfigRender: boolean;
  /** Files whose sidebar shows the import graph (ctx_relations::supported). */
  isRelationsTarget: boolean;
  /** Files whose sidebar pairs them with their tests. */
  isTestsTarget: boolean;
  /** Has a Rendered/Source toggle. */
  isPreviewable: boolean;
}

export function classifyFile(path: string): FileKind {
  const isSvg = /\.svg$/i.test(path);
  const isHtml = /\.x?html?$/i.test(path);
  const isImage = /\.(png|jpe?g|gif|webp|avif|bmp|ico|apng)$/i.test(path);
  const isPdf = /\.pdf$/i.test(path);
  const isMarkdown = /\.(md|markdown|mdx)$/i.test(path);
  const isCss = /\.(css|scss|sass|less|pcss|postcss)$/i.test(path);
  const isJson = /\.json$/i.test(path);
  // SVG is XML but has its own preview path; .xhtml/.html are handled by
  // isHtml. The remaining XML dialects belong to the XML insights sidebar.
  const isXml = !isSvg && !isHtml && /\.(xml|xsd|xsl|xslt|rss|atom|plist)$/i.test(path);
  const isYaml = /\.ya?ml$/i.test(path);
  const isSql = /\.sql$/i.test(path);
  // Build files identify by basename (no extension) as well as by suffix:
  // `Makefile`/`GNUmakefile`/`*.mk`, `Dockerfile`/`Dockerfile.dev`/`*.dockerfile`.
  const baseName = path.split('/').pop() ?? '';
  const isMakefile =
    /^(GNUmakefile|[Mm]akefile)$/.test(baseName) || /\.(mk|make|mak)$/i.test(baseName);
  const isDockerfile =
    /^(Dockerfile|Containerfile)(\.[\w.-]+)?$/i.test(baseName) || /\.dockerfile$/i.test(baseName);
  const isToml = /\.toml$/i.test(path);
  const isMmd = /\.(mmd|mermaid)$/i.test(path);
  const configRenderKind: ConfigRenderKind = isMakefile
    ? 'makefile'
    : isDockerfile
      ? 'dockerfile'
      : isYaml
        ? 'yaml'
        : isJson
          ? 'json'
          : isToml
            ? 'toml'
            : isXml
              ? 'xml'
              : '';
  const isConfigRender = configRenderKind !== '';
  return {
    isSvg,
    isHtml,
    isImage,
    isPdf,
    isBinaryView: isImage || isPdf,
    isMarkdown,
    isCss,
    isJson,
    isXml,
    isYaml,
    isSql,
    isMakefile,
    isDockerfile,
    isToml,
    isMmd,
    configRenderKind,
    isConfigRender,
    isRelationsTarget: /\.(go|rs|tsx?|jsx?|mjs|cjs|svelte|vue|py|java|kts?|php|swift)$/i.test(path),
    // Go gets symbol-aware test insight and coverage; every other language
    // pairs files with their tests by naming convention and imports.
    isTestsTarget:
      /\.(go|rs|tsx?|jsx?|mjs|cjs|svelte|vue|py|java|kts?|php|swift|rb|cs|scala|dart|exs?|c|cc|cpp|h|hpp)$/i.test(
        path,
      ),
    isPreviewable: isSvg || isHtml || isMarkdown || isMmd || isConfigRender,
  };
}
