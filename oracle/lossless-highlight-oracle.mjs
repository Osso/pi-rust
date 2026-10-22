import { createInterface } from 'node:readline';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

const sourceUrl = pathToFileURL(process.argv[2]);
const api = await import(sourceUrl.href);
const html = await import(new URL('./html.ts', sourceUrl).href);
const hljs = createRequire(process.argv[2])('highlight.js/lib/index.js');
const string = units => String.fromCharCode(...units);
const units = text => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const scopes = [
  'keyword', 'string', 'number', 'comment', 'title', 'attr', 'literal',
  'built_in', 'meta', 'type', 'variable', 'function', 'default',
];
const theme = Object.fromEntries(scopes.map(scope => [scope, text => `<${scope}>${text}</${scope}>`]));

function highlightCase(input, code) {
  const language = input.language === null ? undefined : string(input.language);
  const options = {
    language,
    ignoreIllegals: true,
    languageSubset: input.subset?.map(string),
    theme,
  };
  try {
    const highlighted = input.kind === 'raw'
      ? hljs.highlight(code, options).value
      : api.highlight(code, options);
    return units(highlighted);
  } catch (error) {
    return { error: units(error.message) };
  }
}

function evaluateCase(input) {
  const code = string(input.code);
  if (input.kind === 'supports') return api.supportsLanguage(code);
  if (input.kind === 'render') return units(api.renderHighlightedHtml(code, theme));
  if (input.kind === 'entity') {
    const decoded = html.decodeHtmlEntity(code);
    return decoded === undefined ? null : units(decoded);
  }
  if (input.kind === 'entity_at') {
    const decoded = html.decodeHtmlEntityAt(code, input.index);
    return decoded === undefined ? null : { text: units(decoded.text), length: decoded.length };
  }
  return highlightCase(input, code);
}

for await (const line of createInterface({ input: process.stdin })) {
  const cases = JSON.parse(line).cases;
  console.log(JSON.stringify(cases.map(evaluateCase)));
}
