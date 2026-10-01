#!/usr/bin/env bun

/**
 * extract.mjs — Extract testRule() cases from Stylelint repos into test-cases.json
 *
 * Usage:
 *   bun extract.mjs                          # Clone repos and extract
 *   bun extract.mjs --no-clone               # Skip cloning (use existing .clones/)
 *   bun extract.mjs --update                 # Pull the latest upstream into existing clones
 *   bun extract.mjs --with <name>            # Also extract an optional repo (see REPOS)
 *   bun extract.mjs --verbose                # Show detailed extraction info
 *
 * The test files are read statically, never executed.  A small evaluator
 * understands the subset of JavaScript the upstream suites use for their test
 * data: string and template literals (with exact escape handling, since fix
 * output is compared byte-for-byte), the `stripIndent` and `dedent` template
 * tags, `+` concatenation, top-level `const` references and `...spread`, and
 * `mergeTestDescriptions(shared, {...})`.  Anything else (function calls,
 * `messages.rejected(...)`, loops) is treated as unknown and dropped.
 */

import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const CLONES_DIR = join(__dirname, ".clones");
const OUTPUT_FILE = join(__dirname, "test-cases.json");

const argv = process.argv.slice(2);
const VERBOSE = argv.includes("--verbose");
const SKIP_CLONE = argv.includes("--no-clone");
const UPDATE = argv.includes("--update");
const WITH = new Set(
  argv.flatMap((arg, i) => (argv[i - 1] === "--with" ? arg.split(",") : [])).map((s) => s.trim()),
);

// ---------------------------------------------------------------------------
// Repos to clone
// ---------------------------------------------------------------------------
//
// Adding a repo is one entry here:
//   name        directory under .clones/ and the `source` field in test-cases.json
//   repo        GitHub owner/name, shallow-cloned at `branch`
//   testGlob    test files, relative to the clone; `*` matches within one path
//               segment, and the segment that is exactly `*` names the rule
//   rulePrefix  prepended to that directory name (`scss/`, `order/`, ...)
//   optional    skipped unless asked for with `--with <name>`
//   fixWithoutFlag
//               jest-preset-stylelint only checks `fixed` when the testRule()
//               block sets `fix: true`; set this for test runners (such as
//               @morev/stylelint-testing-library) that check `fixed` always
//   templateTags
//               per-repo overrides for tagged-template helpers (see TAGS)

const REPOS = [
  {
    name: "stylelint",
    repo: "stylelint/stylelint",
    branch: "main",
    testGlob: "lib/rules/*/__tests__/index.mjs",
    rulePrefix: "",
  },
  {
    name: "stylelint-scss",
    repo: "stylelint-scss/stylelint-scss",
    branch: "master",
    testGlob: "src/rules/*/__tests__/index.js",
    rulePrefix: "scss/",
    // jest-setup.js defines a global `dedent` that differs from the npm package.
    templateTags: { dedent: "scss-dedent" },
  },
  {
    name: "stylelint-order",
    repo: "hudochenkov/stylelint-order",
    branch: "master",
    testGlob: "rules/*/tests/*.js",
    rulePrefix: "order/",
  },
  // @stylistic/stylelint-plugin is not extracted yet.  Its entry would be
  //   name: "stylelint-stylistic", repo: "stylelint-stylistic/stylelint-stylistic",
  //   branch: "main", testGlob: "lib/rules/*/*.test.ts", rulePrefix: "@stylistic/",
  //   optional: true, fixWithoutFlag: true,
  // but its suites run on @morev/stylelint-testing-library with
  // `autoStripIndent: true` (overridable per file, group and case), so `code`
  // and `fixed` need that library's stripIndent applied before they are
  // byte-exact.  Add that, then audit the output before dropping `optional`.
];

// ---------------------------------------------------------------------------
// Cloning
// ---------------------------------------------------------------------------

function git(args, cwd) {
  return execSync(`git ${args}`, { cwd, stdio: "pipe", timeout: 300_000 }).toString().trim();
}

function cloneRepo(repoConfig) {
  const dest = join(CLONES_DIR, repoConfig.name);

  if (existsSync(dest)) {
    if (UPDATE) {
      console.log(`  [update] ${repoConfig.repo} @ ${repoConfig.branch}`);
      try {
        git(`fetch --depth 1 origin ${repoConfig.branch}`, dest);
        git("reset --hard FETCH_HEAD", dest);
      } catch (e) {
        console.error(`  [error] Update failed: ${e.message}`);
      }
    } else {
      console.log(`  [skip] Already cloned: ${repoConfig.name}`);
    }
    return dest;
  }

  console.log(`  [clone] ${repoConfig.repo} @ ${repoConfig.branch}`);
  try {
    git(
      `clone --depth 1 --branch ${repoConfig.branch} https://github.com/${repoConfig.repo}.git ${dest}`,
    );
  } catch (e) {
    console.error(`  [error] Clone failed: ${e.message}`);
    return null;
  }

  return dest;
}

function headCommit(cloneDir) {
  try {
    return git("rev-parse --short HEAD", cloneDir);
  } catch {
    return "unknown";
  }
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

function segmentMatcher(pattern) {
  if (!pattern.includes("*")) return (name) => name === pattern;
  const re = new RegExp(
    "^" + pattern.split("*").map((s) => s.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join("[^/]*") + "$",
  );
  return (name) => re.test(name);
}

/** Returns [{ path, rule }] for every file matching `globPattern`. */
function findTestFiles(cloneDir, globPattern) {
  const parts = globPattern.split("/");
  const matchers = parts.map(segmentMatcher);
  const ruleSegment = parts.indexOf("*");
  const files = [];

  function walk(dir, depth, captured) {
    let entries;
    try {
      entries = readdirSync(dir).sort();
    } catch {
      return;
    }
    const isLast = depth === parts.length - 1;
    for (const entry of entries) {
      if (!matchers[depth](entry)) continue;
      const fullPath = join(dir, entry);
      const next = depth === ruleSegment ? entry : captured;
      let stat;
      try {
        stat = statSync(fullPath);
      } catch {
        continue;
      }
      if (isLast) {
        if (stat.isFile()) files.push({ path: fullPath, rule: next });
      } else if (stat.isDirectory()) {
        walk(fullPath, depth + 1, next);
      }
    }
  }

  walk(cloneDir, 0, null);
  return files;
}

// ---------------------------------------------------------------------------
// A tiny static evaluator for test-file object literals
// ---------------------------------------------------------------------------

/** A value the evaluator could not work out statically (a call, a loop variable, ...). */
class Unknown {
  constructor(raw) {
    this.raw = raw;
  }
}
const isUnknown = (v) => v instanceof Unknown;
const isPlainObject = (v) => v !== null && typeof v === "object" && !Array.isArray(v) && !isUnknown(v);

/** Source offset of every object literal, so cases can point back at their upstream line. */
const OBJECT_OFFSETS = new WeakMap();

const IDENT_START = /[A-Za-z_$]/;
const IDENT_CHAR = /[\w$]/;

function skipTrivia(t, i) {
  for (;;) {
    while (i < t.length && /\s/.test(t[i])) i++;
    if (t[i] === "/" && t[i + 1] === "/") {
      const e = t.indexOf("\n", i);
      i = e < 0 ? t.length : e + 1;
      continue;
    }
    if (t[i] === "/" && t[i + 1] === "*") {
      const e = t.indexOf("*/", i + 2);
      i = e < 0 ? t.length : e + 2;
      continue;
    }
    return i;
  }
}

/** Decodes one escape sequence at t[i] === "\\" with JavaScript's cooked-string rules. */
function readEscape(t, i) {
  const c = t[i + 1];
  const simple = { n: "\n", t: "\t", r: "\r", b: "\b", f: "\f", v: "\v" };
  if (c in simple) return { str: simple[c], end: i + 2 };
  if (c === "0" && !/[0-9]/.test(t[i + 2] ?? "")) return { str: "\0", end: i + 2 };
  if (c === "x") {
    return { str: String.fromCharCode(parseInt(t.slice(i + 2, i + 4), 16)), end: i + 4 };
  }
  if (c === "u") {
    if (t[i + 2] === "{") {
      const close = t.indexOf("}", i + 3);
      return { str: String.fromCodePoint(parseInt(t.slice(i + 3, close), 16)), end: close + 1 };
    }
    return { str: String.fromCharCode(parseInt(t.slice(i + 2, i + 6), 16)), end: i + 6 };
  }
  // Line continuations contribute nothing.
  if (c === "\r") return { str: "", end: t[i + 2] === "\n" ? i + 3 : i + 2 };
  if (c === "\n" || c === "\u2028" || c === "\u2029") return { str: "", end: i + 2 };
  return { str: c, end: i + 2 };
}

function parseQuoted(t, i) {
  const q = t[i];
  let out = "";
  i++;
  while (i < t.length && t[i] !== q) {
    if (t[i] === "\\") {
      const e = readEscape(t, i);
      out += e.str;
      i = e.end;
      continue;
    }
    out += t[i++];
  }
  return { value: out, end: i + 1 };
}

/** Parses a template literal at t[i] === "`" into cooked and raw chunks plus evaluated substitutions. */
function parseTemplate(ctx, i) {
  const t = ctx.text;
  const cooked = [];
  const raw = [];
  const exprs = [];
  let c = "";
  let r = "";
  i++;
  while (i < t.length) {
    const ch = t[i];
    if (ch === "`") {
      cooked.push(c);
      raw.push(r);
      return { cooked, raw, exprs, end: i + 1 };
    }
    if (ch === "\\") {
      const e = readEscape(t, i);
      c += e.str;
      r += t.slice(i, e.end).replace(/\r\n?/g, "\n");
      i = e.end;
      continue;
    }
    if (ch === "$" && t[i + 1] === "{") {
      cooked.push(c);
      raw.push(r);
      c = "";
      r = "";
      const v = parseExpression(ctx, i + 2);
      const j = skipTrivia(t, v.end);
      if (t[j] === "}") {
        exprs.push(v.value);
        i = j + 1;
      } else {
        exprs.push(new Unknown(""));
        const close = scanBalanced(t, i + 1);
        i = close < 0 ? t.length : close + 1;
      }
      continue;
    }
    // Template literals normalise CRLF and CR line terminators to LF.
    if (ch === "\r") {
      c += "\n";
      r += "\n";
      i += t[i + 1] === "\n" ? 2 : 1;
      continue;
    }
    c += ch;
    r += ch;
    i++;
  }
  return { cooked, raw, exprs, end: t.length };
}

function interpolate(strings, exprs) {
  if (exprs.some((e) => typeof e !== "string" && typeof e !== "number" && typeof e !== "boolean")) {
    return new Unknown("template");
  }
  let out = strings[0];
  for (let k = 0; k < exprs.length; k++) out += String(exprs[k]) + strings[k + 1];
  return out;
}

// Tagged-template helpers, reproduced exactly because fix output is compared
// byte-for-byte.
const TAGS = {
  // common-tags@1.8 stripIndent: drop the smallest indentation, then trim.
  stripIndent(strings, exprs) {
    const s = interpolate(strings, exprs);
    if (isUnknown(s)) return s;
    const match = s.match(/^[^\S\n]*(?=\S)/gm);
    const indent = match && Math.min(...match.map((el) => el.length));
    const stripped = indent ? s.replace(new RegExp(`^.{${indent}}`, "gm"), "") : s;
    return stripped.trim();
  },
  // common-tags@1.8 stripIndents: drop all indentation, then trim.
  stripIndents(strings, exprs) {
    const s = interpolate(strings, exprs);
    if (isUnknown(s)) return s;
    return s.replace(/^[^\S\n]+/gm, "").trim();
  },
  // stylelint-scss jest-setup.js global: only looks at the first chunk.
  "scss-dedent"(strings) {
    return strings[0].replace(/^[ \t]+/gm, "").replace(/^\s*\n/gm, "");
  },
  "String.raw"(_strings, exprs, raw) {
    return interpolate(raw, exprs);
  },
};

/** deepmerge's default semantics, which mergeTestDescriptions relies on. */
function deepMerge(target, source) {
  if (Array.isArray(target) && Array.isArray(source)) return [...target, ...source];
  if (isPlainObject(target) && isPlainObject(source)) {
    const out = { ...target };
    for (const key of Object.keys(source)) {
      out[key] = key in target ? deepMerge(target[key], source[key]) : source[key];
    }
    return out;
  }
  return source;
}

const CALLS = {
  mergeTestDescriptions(args) {
    if (!args.every(isPlainObject)) return new Unknown("mergeTestDescriptions");
    return args.reduce((acc, a) => deepMerge(acc, a), {});
  },
};

function skipString(t, i) {
  const q = t[i];
  i++;
  while (i < t.length && t[i] !== q) {
    if (t[i] === "\\") i++;
    i++;
  }
  return i + 1;
}

function skipTemplate(t, i) {
  i++;
  while (i < t.length && t[i] !== "`") {
    if (t[i] === "\\") {
      i += 2;
      continue;
    }
    if (t[i] === "$" && t[i + 1] === "{") {
      const close = scanBalanced(t, i + 1);
      i = close < 0 ? t.length : close + 1;
      continue;
    }
    i++;
  }
  return i + 1;
}

/** True when a `/` at t[i] starts a regex literal rather than a division. */
function startsRegex(t, i) {
  let k = i - 1;
  while (k >= 0 && /\s/.test(t[k])) k--;
  return k < 0 || "(,=:[!&|?{};+-*%<>~^".includes(t[k]);
}

function skipRegex(t, i) {
  let inClass = false;
  i++;
  while (i < t.length) {
    const ch = t[i];
    if (ch === "\\") {
      i += 2;
      continue;
    }
    if (ch === "[") inClass = true;
    else if (ch === "]") inClass = false;
    else if (ch === "/" && !inClass) break;
    else if (ch === "\n") return i;
    i++;
  }
  i++;
  while (i < t.length && /[a-z]/.test(t[i])) i++;
  return i;
}

/** Advances past strings, templates, comments and regex literals at t[i], or returns -1. */
function skipAtom(t, i) {
  const ch = t[i];
  if (ch === "'" || ch === '"') return skipString(t, i);
  if (ch === "`") return skipTemplate(t, i);
  if (ch === "/" && (t[i + 1] === "/" || t[i + 1] === "*")) return skipTrivia(t, i);
  if (ch === "/" && startsRegex(t, i)) return skipRegex(t, i);
  return -1;
}

/** Given t[i] in `([{`, returns the index of its matching closer (or -1). */
function scanBalanced(t, i) {
  let depth = 0;
  while (i < t.length) {
    const next = skipAtom(t, i);
    if (next >= 0) {
      i = next;
      continue;
    }
    const ch = t[i];
    if (ch === "(" || ch === "[" || ch === "{") depth++;
    else if (ch === ")" || ch === "]" || ch === "}") {
      depth--;
      if (depth === 0) return i;
    }
    i++;
  }
  return -1;
}

/** Returns the index of the `,` `;` or closing bracket that ends the expression starting at i. */
function skipExpression(t, i) {
  while (i < t.length) {
    const next = skipAtom(t, i);
    if (next >= 0) {
      i = next;
      continue;
    }
    const ch = t[i];
    if (ch === "(" || ch === "[" || ch === "{") {
      const close = scanBalanced(t, i);
      if (close < 0) return t.length;
      i = close + 1;
      continue;
    }
    if (ch === "," || ch === ";" || ch === ")" || ch === "]" || ch === "}") return i;
    i++;
  }
  return i;
}

function wordAt(t, i) {
  if (!IDENT_START.test(t[i] ?? "")) return null;
  let j = i + 1;
  while (j < t.length && IDENT_CHAR.test(t[j])) j++;
  return t.slice(i, j);
}

/** True when the expression that ended before index j is complete. */
function expressionEndsAt(t, from, j) {
  if (j >= t.length || ",;)]}".includes(t[j])) return true;
  // Automatic semicolon insertion: a new statement on the next line.
  return t.slice(from, j).includes("\n") && IDENT_START.test(t[j]);
}

function parseExpression(ctx, i) {
  const t = ctx.text;
  const start = skipTrivia(t, i);
  let res = parsePrimary(ctx, start);
  if (res) res = parsePostfix(ctx, res);

  if (res) {
    let j = skipTrivia(t, res.end);
    // String / number concatenation.
    while (res && t[j] === "+" && t[j + 1] !== "+" && t[j + 1] !== "=") {
      let rhs = parsePrimary(ctx, skipTrivia(t, j + 1));
      if (rhs) rhs = parsePostfix(ctx, rhs);
      if (!rhs) {
        res = null;
        break;
      }
      const ok = (v) => typeof v === "string" || typeof v === "number";
      res = {
        value: ok(res.value) && ok(rhs.value) ? res.value + rhs.value : new Unknown("+"),
        end: rhs.end,
      };
      j = skipTrivia(t, res.end);
    }
    if (res) {
      // TypeScript `as const` / `satisfies T` leave the value alone.
      const word = wordAt(t, j);
      if ((word === "as" || word === "satisfies") && !t.slice(res.end, j).includes("\n")) {
        return { value: res.value, end: skipExpression(t, j) };
      }
      if (expressionEndsAt(t, res.end, j)) return { value: res.value, end: res.end };
    }
  }

  const end = skipExpression(t, start);
  return { value: new Unknown(t.slice(start, end).trim()), end };
}

function parsePrimary(ctx, i) {
  const t = ctx.text;
  const ch = t[i];
  if (ch === undefined) return null;

  if (ch === "'" || ch === '"') return parseQuoted(t, i);

  if (ch === "`") {
    const tpl = parseTemplate(ctx, i);
    return { value: interpolate(tpl.cooked, tpl.exprs), end: tpl.end };
  }

  const num = /^[-+]?(?:0[xX][\da-fA-F]+|(?:\d[\d_]*\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)/.exec(
    t.slice(i, i + 64),
  );
  if (num && (/[\d.]/.test(ch) || /[\d.]/.test(t[i + 1] ?? ""))) {
    return { value: Number(num[0].replace(/_/g, "")), end: i + num[0].length };
  }

  if (ch === "[") return parseArray(ctx, i);
  if (ch === "{") return parseObject(ctx, i);

  if (ch === "(") {
    const close = scanBalanced(t, i);
    if (close < 0) return null;
    const after = skipTrivia(t, close + 1);
    if (t.startsWith("=>", after)) return null; // arrow function
    const inner = parseExpression(ctx, i + 1);
    if (skipTrivia(t, inner.end) !== close) return null;
    return { value: inner.value, end: close + 1 };
  }

  if (ch === "/" && startsRegex(t, i)) {
    const end = skipRegex(t, i);
    return { value: t.slice(i, end), end };
  }

  const word = wordAt(t, i);
  if (!word) return null;
  let end = i + word.length;

  const literals = { true: true, false: false, null: null, undefined: undefined };
  if (word in literals) return { value: literals[word], end };
  if (["new", "function", "async", "class", "typeof", "void", "await", "yield"].includes(word)) {
    return null;
  }

  // `String.raw` is the one member expression used as a template tag.
  let name = word;
  if (word === "String" && t.startsWith(".raw", end) && t[skipTrivia(t, end + 4)] === "`") {
    name = "String.raw";
    end += 4;
  }

  const after = skipTrivia(t, end);
  if (t.startsWith("=>", after)) return null;

  if (t[after] === "`") {
    const tpl = parseTemplate(ctx, after);
    const tag = TAGS[ctx.templateTags[name] ?? name];
    const value = tag ? tag(tpl.cooked, tpl.exprs, tpl.raw) : new Unknown(name);
    return { value, end: tpl.end };
  }

  if (t[after] === "(") {
    const close = scanBalanced(t, after);
    if (close < 0) return null;
    const fn = CALLS[name];
    const value = fn ? fn(parseArgs(ctx, after, close)) : new Unknown(t.slice(i, close + 1));
    return { value, end: close + 1 };
  }

  return { value: resolveIdentifier(ctx, word), end };
}

/** Evaluates the comma-separated arguments between t[open] === "(" and t[close]. */
function parseArgs(ctx, open, close) {
  const t = ctx.text;
  const args = [];
  let j = open + 1;
  for (;;) {
    j = skipTrivia(t, j);
    if (j >= close) return args;
    const v = parseExpression(ctx, j);
    args.push(v.value);
    j = skipTrivia(t, v.end);
    if (t[j] !== ",") return args;
    j++;
  }
}

/**
 * Applies member access and calls after a primary: `shared.accept`,
 * `shared.accept.concat([...])`, `messages.rejected('x')` (unknown).
 */
function parsePostfix(ctx, res) {
  const t = ctx.text;
  for (;;) {
    const j = skipTrivia(t, res.end);
    if (t[j] === "." && t[j + 1] !== ".") {
      const at = skipTrivia(t, j + 1);
      const name = wordAt(t, at);
      if (!name) return null;
      const base = res.value;
      const after = skipTrivia(t, at + name.length);
      if (t[after] === "(") {
        const close = scanBalanced(t, after);
        if (close < 0) return null;
        const args = parseArgs(ctx, after, close);
        const value =
          name === "concat" && Array.isArray(base)
            ? base.concat(...args.map((a) => (Array.isArray(a) ? a : [a])))
            : new Unknown(`.${name}()`);
        res = { value, end: close + 1 };
        continue;
      }
      const value = isPlainObject(base) ? (Object.hasOwn(base, name) ? base[name] : undefined) : new Unknown(`.${name}`);
      res = { value, end: at + name.length };
      continue;
    }
    if (t[j] === "[" && !t.slice(res.end, j).includes("\n")) {
      const close = scanBalanced(t, j);
      if (close < 0) return null;
      const k = parseExpression(ctx, j + 1).value;
      const base = res.value;
      const ok =
        (isPlainObject(base) && typeof k === "string" && Object.hasOwn(base, k)) ||
        (Array.isArray(base) && Number.isInteger(k));
      res = { value: ok ? base[k] : new Unknown("[]"), end: close + 1 };
      continue;
    }
    return res;
  }
}

function parseArray(ctx, i) {
  const t = ctx.text;
  const out = [];
  let j = i + 1;
  for (;;) {
    j = skipTrivia(t, j);
    if (j >= t.length) return null;
    if (t[j] === "]") return { value: out, end: j + 1 };
    if (t[j] === ",") {
      j++;
      continue;
    }
    if (t.startsWith("...", j)) {
      const v = parseExpression(ctx, j + 3);
      if (Array.isArray(v.value)) out.push(...v.value);
      else out.push(new Unknown(`...${isUnknown(v.value) ? v.value.raw : ""}`));
      j = v.end;
      continue;
    }
    const v = parseExpression(ctx, j);
    out.push(v.value);
    j = skipTrivia(t, v.end);
    if (t[j] === ",") j++;
    else if (t[j] !== "]") {
      const close = scanBalanced(t, i);
      return close < 0 ? null : { value: out, end: close + 1 };
    }
  }
}

function parseObject(ctx, i) {
  const t = ctx.text;
  const obj = {};
  OBJECT_OFFSETS.set(obj, { ctx, offset: i });
  const bail = () => {
    const close = scanBalanced(t, i);
    return close < 0 ? null : { value: obj, end: close + 1 };
  };
  let j = i + 1;
  for (;;) {
    j = skipTrivia(t, j);
    if (j >= t.length) return null;
    if (t[j] === "}") return { value: obj, end: j + 1 };
    if (t[j] === ",") {
      j++;
      continue;
    }
    if (t.startsWith("...", j)) {
      const v = parseExpression(ctx, j + 3);
      if (isPlainObject(v.value)) Object.assign(obj, v.value);
      j = v.end;
      continue;
    }

    let key = null;
    if (t[j] === "'" || t[j] === '"') {
      const s = parseQuoted(t, j);
      key = s.value;
      j = s.end;
    } else if (t[j] === "[") {
      // Computed key: `[/^foo/]: 1` keys the object by the regex's source.
      const close = scanBalanced(t, j);
      if (close < 0) return null;
      const k = parseExpression(ctx, j + 1);
      const resolved = typeof k.value === "string" || typeof k.value === "number";
      key = resolved && skipTrivia(t, k.end) === close ? String(k.value) : null;
      // Keep the hole visible to hasUnknown() so a config with it is skipped.
      if (key === null) obj[`[${t.slice(j + 1, close)}]`] = new Unknown("computed key");
      j = close + 1;
    } else {
      key = wordAt(t, j) ?? /^\d+/.exec(t.slice(j, j + 32))?.[0] ?? null;
      if (key === null) return bail();
      j += key.length;
    }

    j = skipTrivia(t, j);
    if (t[j] === ":") {
      const v = parseExpression(ctx, j + 1);
      if (key !== null) obj[key] = v.value;
      j = v.end;
    } else if (t[j] === "(") {
      // Method shorthand: skip the parameters and the body.
      const close = scanBalanced(t, j);
      const body = skipTrivia(t, close + 1);
      if (close < 0 || t[body] !== "{") return bail();
      j = scanBalanced(t, body) + 1;
    } else if (t[j] === "," || t[j] === "}") {
      if (key !== null) obj[key] = resolveIdentifier(ctx, key);
    } else {
      return bail();
    }

    j = skipTrivia(t, j);
    if (t[j] === ",") j++;
    else if (t[j] !== "}") return bail();
  }
}

/** Resolves a top-level `const NAME = ...` in the same file, or returns Unknown. */
function resolveIdentifier(ctx, name) {
  const entry = ctx.scope.get(name);
  if (!entry) return new Unknown(name);
  if ("value" in entry) return entry.value;
  if (entry.resolving) return new Unknown(name);
  entry.resolving = true;
  entry.value = parseExpression(ctx, entry.offset).value;
  entry.resolving = false;
  return entry.value;
}

function buildScope(text) {
  const scope = new Map();
  const re = /^(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=\n]+)?=(?!=)\s*/gm;
  let m;
  while ((m = re.exec(text)) !== null) {
    if (!scope.has(m[1])) scope.set(m[1], { offset: m.index + m[0].length });
  }
  return scope;
}

function lineAt(text, offset) {
  let line = 1;
  for (let k = 0; k < offset && k < text.length; k++) if (text[k] === "\n") line++;
  return line;
}

/** Drops Unknown values so the result is plain JSON. */
function toJSON(v) {
  if (isUnknown(v) || v === undefined) return undefined;
  if (Array.isArray(v)) return v.map(toJSON).filter((x) => x !== undefined);
  if (isPlainObject(v)) {
    const out = {};
    for (const [k, val] of Object.entries(v)) {
      const j = toJSON(val);
      if (j !== undefined) out[k] = j;
    }
    return out;
  }
  return v;
}

function hasUnknown(v) {
  if (isUnknown(v)) return true;
  if (Array.isArray(v)) return v.some(hasUnknown);
  if (isPlainObject(v)) return Object.values(v).some(hasUnknown);
  return false;
}

// ---------------------------------------------------------------------------
// testRule block extraction
// ---------------------------------------------------------------------------

/**
 * Maps a customSyntax value to the syntax Gale parses, or null to skip the
 * group (HTML, CSS-in-JS, SugarSS, or a value that is not a plain string).
 */
function determineSyntax(customSyntax) {
  if (customSyntax === undefined) return "css";
  if (typeof customSyntax !== "string") return null;
  const syntax = {
    "postcss-scss": "scss",
    "postcss-less": "less",
    "postcss-sass": "sass",
  }[customSyntax.toLowerCase()];
  return syntax ?? null;
}

const num = (v) => (typeof v === "number" ? v : undefined);
const str = (v) => (typeof v === "string" ? v : undefined);

function extractCase(item, type, stats, fixGroup) {
  if (!isPlainObject(item) || typeof item.code !== "string") {
    stats.unknownCases++;
    return null;
  }
  if (item.skip === true) {
    stats.skippedCases++;
    return null;
  }

  const entry = { type, code: item.code };
  if (str(item.description)) entry.description = item.description;
  const loc = OBJECT_OFFSETS.get(item);
  if (loc) entry.sourceLine = lineAt(loc.ctx.text, loc.offset);

  if (type === "reject") {
    for (const key of ["line", "column", "endLine", "endColumn"]) {
      if (num(item[key]) !== undefined) entry[key] = item[key];
    }
    if (str(item.message)) entry.message = item.message;
    if (typeof item.fixed === "string") entry.fixed = item.fixed;
    else if ("fixed" in item && fixGroup) stats.unknownFixed++;
    if (item.unfixable === true) entry.unfixable = true;
    if (item.computeEditInfo === true) entry.computeEditInfo = true;
    if (isPlainObject(item.fix)) entry.fix = toJSON(item.fix);
    if (Array.isArray(item.warnings)) entry.warnings = toJSON(item.warnings);
  }
  return entry;
}

/**
 * Find and extract all testRule() blocks from a file's text content.
 */
function extractTestRuleBlocks(fileText, relPath, ruleName, repoConfig, stats) {
  const ctx = {
    text: fileText,
    scope: buildScope(fileText),
    templateTags: repoConfig.templateTags ?? {},
  };
  const groups = [];
  const re = /(?<![\w$.])testRule\s*\(/g;
  let match;

  while ((match = re.exec(fileText)) !== null) {
    const { value: block, end } = parseExpression(ctx, match.index + match[0].length);
    re.lastIndex = Math.max(re.lastIndex, end);
    const where = `${relPath}:${lineAt(fileText, match.index)}`;

    if (!isPlainObject(block)) {
      stats.unknownGroups++;
      if (VERBOSE) console.log(`    [skip] ${where}: testRule() argument is not a static object`);
      continue;
    }
    if (block.skip === true) {
      stats.skippedGroups++;
      continue;
    }

    const blockRuleName = typeof block.ruleName === "string" && block.ruleName ? block.ruleName : ruleName;
    if (!blockRuleName) {
      if (VERBOSE) console.log(`    [warn] ${where}: no ruleName`);
      continue;
    }

    const syntax = determineSyntax(block.customSyntax);
    if (syntax === null) {
      stats.syntaxSkipped++;
      if (VERBOSE) {
        const label = isUnknown(block.customSyntax) ? block.customSyntax.raw : JSON.stringify(block.customSyntax);
        console.log(`    [skip] ${where}: unsupported customSyntax ${label}`);
      }
      continue;
    }

    // An unresolvable config would run the rule with the wrong options.
    if (isUnknown(block.config) || hasUnknown(block.config)) {
      stats.configSkipped++;
      if (VERBOSE) console.log(`    [skip] ${where}: config is not static`);
      continue;
    }
    const config = block.config === undefined ? true : block.config;

    const accept = Array.isArray(block.accept) ? block.accept : [];
    const reject = Array.isArray(block.reject) ? block.reject : [];
    const hasFixData = reject.some((c) => isPlainObject(c) && ("fixed" in c || c.unfixable === true));
    const fix = block.fix === true || (repoConfig.fixWithoutFlag === true && hasFixData);

    const cases = [
      ...accept.map((c) => extractCase(c, "accept", stats, fix)),
      ...reject.map((c) => extractCase(c, "reject", stats, fix)),
    ].filter(Boolean);

    if (cases.length === 0) {
      if (VERBOSE) console.log(`    [skip] ${where}: no static accept/reject cases`);
      continue;
    }

    const group = {
      source: repoConfig.name,
      rule: blockRuleName,
      config,
      syntax,
      file: relPath,
      line: lineAt(fileText, match.index),
    };
    if (fix) group.fix = true;
    if (block.computeEditInfo === true) group.computeEditInfo = true;
    group.cases = cases;
    groups.push(group);
  }

  return groups;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function countFixCases(groups) {
  let fixed = 0;
  let unfixable = 0;
  for (const g of groups) {
    if (!g.fix) continue;
    for (const c of g.cases) {
      if (c.type !== "reject") continue;
      if (c.unfixable) unfixable++;
      else if (typeof c.fixed === "string") fixed++;
    }
  }
  return { fixed, unfixable };
}

function main() {
  console.log("Stylelint Compatibility Test Extractor");
  console.log("======================================\n");

  for (const name of WITH) {
    if (!REPOS.some((r) => r.name === name)) {
      console.error(`[error] Unknown repo for --with: ${name}`);
      process.exit(1);
    }
  }

  mkdirSync(CLONES_DIR, { recursive: true });

  const repos = REPOS.filter((r) => !r.optional || WITH.has(r.name));
  const allGroups = [];
  const stats = {
    unknownGroups: 0,
    skippedGroups: 0,
    syntaxSkipped: 0,
    configSkipped: 0,
    unknownCases: 0,
    skippedCases: 0,
    unknownFixed: 0,
  };

  for (const repoConfig of repos) {
    console.log(`\nProcessing: ${repoConfig.name}`);
    console.log("-".repeat(50));

    let cloneDir;
    if (SKIP_CLONE) {
      cloneDir = join(CLONES_DIR, repoConfig.name);
      if (!existsSync(cloneDir)) {
        console.log(`  [error] Clone not found: ${cloneDir}`);
        continue;
      }
    } else {
      cloneDir = cloneRepo(repoConfig);
      if (!cloneDir) continue;
    }
    console.log(`  [commit] ${headCommit(cloneDir)}`);

    const testFiles = findTestFiles(cloneDir, repoConfig.testGlob);
    console.log(`  [files] Found ${testFiles.length} test files`);

    if (testFiles.length === 0) {
      console.log(`  [warn] No test files found matching: ${repoConfig.testGlob}`);
      continue;
    }

    let totalGroups = 0;
    let totalCases = 0;
    const repoGroups = [];

    for (const testFile of testFiles) {
      const ruleName = testFile.rule ? repoConfig.rulePrefix + testFile.rule : null;
      const rel = relative(cloneDir, testFile.path);

      let fileText;
      try {
        fileText = readFileSync(testFile.path, "utf-8");
      } catch {
        if (VERBOSE) console.log(`    [error] Could not read: ${testFile.path}`);
        continue;
      }

      const groups = extractTestRuleBlocks(fileText, rel, ruleName, repoConfig, stats);

      for (const group of groups) {
        allGroups.push(group);
        repoGroups.push(group);
        totalGroups++;
        totalCases += group.cases.length;
      }

      if (VERBOSE && groups.length > 0) {
        console.log(`    ${rel}: ${groups.length} groups, ${groups.reduce((s, g) => s + g.cases.length, 0)} cases`);
      }
    }

    const fixCounts = countFixCases(repoGroups);
    console.log(`  [extracted] ${totalGroups} test groups, ${totalCases} test cases`);
    console.log(`  [fix] ${fixCounts.fixed} fixed cases, ${fixCounts.unfixable} unfixable cases`);
  }

  // Different testRule blocks for the same rule usually differ in config, so
  // groups are kept separate rather than merged.

  writeFileSync(OUTPUT_FILE, JSON.stringify(allGroups, null, 2));

  const ruleSet = new Set(allGroups.map((g) => g.rule));
  const totalCases = allGroups.reduce((s, g) => s + g.cases.length, 0);
  const fixCounts = countFixCases(allGroups);
  const fixRules = new Set(allGroups.filter((g) => g.fix).map((g) => g.rule));

  console.log("\n" + "=".repeat(50));
  console.log("Summary");
  console.log("=".repeat(50));
  console.log(`  Total test groups:   ${allGroups.length}`);
  console.log(`  Total test cases:    ${totalCases}`);
  console.log(`  Unique rules:        ${ruleSet.size}`);
  console.log(`  Fix cases:           ${fixCounts.fixed} fixed + ${fixCounts.unfixable} unfixable across ${fixRules.size} rules`);
  console.log(`  Output:              ${OUTPUT_FILE}`);

  console.log("\n  Not extracted:");
  console.log(`    testRule() blocks that are not static objects: ${stats.unknownGroups}`);
  console.log(`    groups marked skip upstream:                   ${stats.skippedGroups}`);
  console.log(`    groups with an unsupported customSyntax:       ${stats.syntaxSkipped}`);
  console.log(`    groups whose config is not static:             ${stats.configSkipped}`);
  console.log(`    cases whose code is not static:                ${stats.unknownCases}`);
  console.log(`    cases marked skip upstream:                    ${stats.skippedCases}`);
  console.log(`    fix cases whose fixed value is not static:     ${stats.unknownFixed}`);

  for (const repo of repos) {
    const sourceGroups = allGroups.filter((g) => g.source === repo.name);
    const sourceCases = sourceGroups.reduce((s, g) => s + g.cases.length, 0);
    const sourceRules = new Set(sourceGroups.map((g) => g.rule));
    const sourceFix = countFixCases(sourceGroups);
    console.log(`\n  ${repo.name}:`);
    console.log(`    Groups: ${sourceGroups.length}, Cases: ${sourceCases}, Rules: ${sourceRules.size}`);
    console.log(`    Fix cases: ${sourceFix.fixed} fixed, ${sourceFix.unfixable} unfixable`);
  }

  console.log();
}

main();
