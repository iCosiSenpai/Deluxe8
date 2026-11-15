// Checks the Italian dictionary against the UI: every literal tr('...') in src must have a translation, and an English
// key must not get two different translations in different files. Run by `pnpm check`.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const src = join(root, 'src');
const dictDir = join(src, 'lib', 'i18n', 'it');

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

// Italian dictionaries: one `"english": "italiano",` entry per line (JSON strings).
const italian = new Map();
const problems = [];
for (const file of walk(dictDir).filter((f) => f.endsWith('.ts'))) {
  for (const line of readFileSync(file, 'utf8').split('\n')) {
    const m = line.match(/^\s*("(?:[^"\\]|\\.)*")\s*:\s*("(?:[^"\\]|\\.)*"),?\s*$/);
    if (!m) continue;
    const [en, it] = [JSON.parse(m[1]), JSON.parse(m[2])];
    const seen = italian.get(en);
    if (seen && seen.it !== it) {
      problems.push(`"${en}" has two translations: "${seen.it}" (${seen.file}) and "${it}" (${relative(root, file)})`);
    }
    italian.set(en, { it, file: relative(root, file) });
  }
}

// Literal first arguments of tr(...): '...' or "..." (template literals are not allowed: use {placeholders}).
const call = /\btr\(\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)"|(`))/g;
let used = 0;
for (const file of walk(src).filter((f) => /\.(svelte|ts)$/.test(f) && !f.startsWith(dictDir))) {
  const text = readFileSync(file, 'utf8');
  for (const m of text.matchAll(call)) {
    const where = `${relative(root, file)}:${text.slice(0, m.index).split('\n').length}`;
    if (m[3]) {
      problems.push(`${where}: tr() with a template literal; use tr('... {name} ...', { name })`);
      continue;
    }
    const en = (m[1] ?? m[2]).replace(/\\(['"\\])/g, '$1');
    used++;
    if (!italian.has(en)) problems.push(`${where}: no Italian for "${en}"`);
  }
}

if (problems.length) {
  console.error(problems.join('\n'));
  console.error(`\ni18n: ${problems.length} problem(s)`);
  process.exit(1);
}
console.log(`i18n: ${used} strings in the UI, ${italian.size} Italian translations, all covered`);
