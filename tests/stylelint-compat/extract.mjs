#!/usr/bin/env node

/**
 * extract.mjs — Extract testRule() cases from Stylelint repos into test-cases.json
 *
 * Usage:
 *   node extract.mjs              # Clone repos and extract
 *   node extract.mjs --no-clone   # Skip cloning (use existing .clones/)
 *   node extract.mjs --verbose    # Show detailed extraction info
 */

import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const CLONES_DIR = join(__dirname, ".clones");
const OUTPUT_FILE = join(__dirname, "test-cases.json");

const VERBOSE = process.argv.includes("--verbose");
const SKIP_CLONE = process.argv.includes("--no-clone");

// ---------------------------------------------------------------------------
// Repos to clone
// ---------------------------------------------------------------------------

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
  },
  {
    name: "stylelint-order",
    repo: "hudochenkov/stylelint-order",
    branch: "master",
    testGlob: "rules/*/tests/*.js",
    rulePrefix: "order/",
  },
];

// ---------------------------------------------------------------------------
// Cloning
// ---------------------------------------------------------------------------

function cloneRepo(repoConfig) {
  const dest = join(CLONES_DIR, repoConfig.name);

  if (existsSync(dest)) {
    console.log(`  [skip] Already cloned: ${repoConfig.name}`);
    return dest;
  }

  console.log(`  [clone] ${repoConfig.repo} @ ${repoConfig.branch}`);
  try {
    execSync(
      `git clone --depth 1 --branch ${repoConfig.branch} https://github.com/${repoConfig.repo}.git ${dest}`,
      { stdio: "pipe", timeout: 120_000 },
    );
  } catch (e) {
    console.error(`  [error] Clone failed: ${e.message}`);
    return null;
  }

  return dest;
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

function findTestFiles(cloneDir, globPattern) {
  // Simple glob matching: split pattern into directory parts and match
  // e.g., "lib/rules/*/__tests__/index.mjs"
  const files = [];
  const parts = globPattern.split("/");

  function walk(dir, depth) {
    if (depth >= parts.length) return;

    const pattern = parts[depth];
    const isLast = depth === parts.length - 1;

    let entries;
    try {
      entries = readdirSync(dir);
    } catch {
      return;
    }

    for (const entry of entries) {
      const fullPath = join(dir, entry);

      if (pattern === "*" || pattern === "*.js" || pattern === "*.mjs") {
        // Wildcard: match any entry
        if (isLast) {
          // Check extension match
          if (pattern === "*" || fullPath.endsWith(pattern.slice(1))) {
            try {
              if (statSync(fullPath).isFile()) {
                files.push(fullPath);
              }
            } catch {}
          }
        } else {
          try {
            if (statSync(fullPath).isDirectory()) {
              walk(fullPath, depth + 1);
            }
          } catch {}
        }
      } else if (entry === pattern) {
        if (isLast) {
          try {
            if (statSync(fullPath).isFile()) {
              files.push(fullPath);
            }
          } catch {}
        } else {
          walk(fullPath, depth + 1);
        }
      }
    }
  }

  walk(cloneDir, 0);
  return files;
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/**
 * Extract rule name from directory structure.
 * e.g., lib/rules/color-no-invalid-hex/__tests__/index.mjs -> "color-no-invalid-hex"
 * e.g., src/rules/at-rule-no-unknown/__tests__/index.js -> "at-rule-no-unknown"
 * e.g., rules/order/tests/index.js -> "order"
 */
function extractRuleNameFromPath(filePath, cloneDir, repoConfig) {
  const rel = relative(cloneDir, filePath);
  const parts = rel.split("/");

  if (repoConfig.name === "stylelint") {
    // lib/rules/<rule-name>/__tests__/index.mjs
    if (parts.length >= 4 && parts[0] === "lib" && parts[1] === "rules") {
      return repoConfig.rulePrefix + parts[2];
    }
  } else if (repoConfig.name === "stylelint-scss") {
    // src/rules/<rule-name>/__tests__/index.js
    if (parts.length >= 4 && parts[0] === "src" && parts[1] === "rules") {
      return repoConfig.rulePrefix + parts[2];
    }
  } else if (repoConfig.name === "stylelint-order") {
    // rules/<rule-name>/tests/*.js
    if (parts.length >= 3 && parts[0] === "rules") {
      return repoConfig.rulePrefix + parts[1];
    }
  }

  return null;
}

/**
 * Find matching closing brace for a given opening brace position.
 * Handles nested braces, strings, template literals, and comments.
 */
function findMatchingBrace(text, startPos) {
  let depth = 0;
  let i = startPos;
  const len = text.length;

  while (i < len) {
    const ch = text[i];

    // Skip template literals
    if (ch === "`") {
      i++;
      while (i < len && text[i] !== "`") {
        if (text[i] === "\\" ) { i++; }
        if (text[i] === "$" && i + 1 < len && text[i + 1] === "{") {
          // Template expression — find its end recursively
          i += 2;
          let exprDepth = 1;
          while (i < len && exprDepth > 0) {
            if (text[i] === "{") exprDepth++;
            else if (text[i] === "}") exprDepth--;
            else if (text[i] === "`") {
              // Nested template literal
              i++;
              while (i < len && text[i] !== "`") {
                if (text[i] === "\\") i++;
                i++;
              }
            } else if (text[i] === "'" || text[i] === '"') {
              const quote = text[i];
              i++;
              while (i < len && text[i] !== quote) {
                if (text[i] === "\\") i++;
                i++;
              }
            }
            i++;
          }
          continue;
        }
        i++;
      }
      i++;
      continue;
    }

    // Skip single-quoted strings
    if (ch === "'") {
      i++;
      while (i < len && text[i] !== "'") {
        if (text[i] === "\\") i++;
        i++;
      }
      i++;
      continue;
    }

    // Skip double-quoted strings
    if (ch === '"') {
      i++;
      while (i < len && text[i] !== '"') {
        if (text[i] === "\\") i++;
        i++;
      }
      i++;
      continue;
    }

    // Skip line comments
    if (ch === "/" && i + 1 < len && text[i + 1] === "/") {
      i += 2;
      while (i < len && text[i] !== "\n") i++;
      i++;
      continue;
    }

    // Skip block comments
    if (ch === "/" && i + 1 < len && text[i + 1] === "*") {
      i += 2;
      while (i < len && !(text[i] === "*" && i + 1 < len && text[i + 1] === "/")) i++;
      i += 2;
      continue;
    }

    if (ch === "{") {
      depth++;
    } else if (ch === "}") {
      depth--;
      if (depth === 0) {
        return i;
      }
    }

    i++;
  }

  return -1;
}

/**
 * Try to extract a JS value (string, boolean, number, array, object) from text at given position.
 * Returns { value, endPos } or null.
 * This is a best-effort static parser — it handles common patterns but not dynamic expressions.
 */
function extractJSValue(text, pos) {
  // Skip whitespace
  while (pos < text.length && /\s/.test(text[pos])) pos++;

  if (pos >= text.length) return null;

  const ch = text[pos];

  // Template literal
  if (ch === "`") {
    let str = "";
    let i = pos + 1;
    while (i < text.length && text[i] !== "`") {
      if (text[i] === "\\") {
        str += text[i] + text[i + 1];
        i += 2;
        continue;
      }
      if (text[i] === "$" && i + 1 < text.length && text[i + 1] === "{") {
        // Template expression — we can't evaluate it, so return null for dynamic templates
        // But for simple cases like `${"\n"}`, try to continue
        return null;
      }
      str += text[i];
      i++;
    }
    // Process escape sequences in the captured string
    try {
      // Use JSON.parse on a double-quoted version to handle \n, \t etc.
      // But template literals preserve literal newlines, so just return as-is
      return { value: str, endPos: i + 1 };
    } catch {
      return { value: str, endPos: i + 1 };
    }
  }

  // Single or double quoted string
  if (ch === "'" || ch === '"') {
    let str = "";
    let i = pos + 1;
    while (i < text.length && text[i] !== ch) {
      if (text[i] === "\\") {
        const next = text[i + 1];
        if (next === "n") { str += "\n"; i += 2; continue; }
        if (next === "t") { str += "\t"; i += 2; continue; }
        if (next === "\\") { str += "\\"; i += 2; continue; }
        if (next === ch) { str += ch; i += 2; continue; }
        str += next;
        i += 2;
        continue;
      }
      str += text[i];
      i++;
    }
    return { value: str, endPos: i + 1 };
  }

  // Boolean
  if (text.startsWith("true", pos)) {
    return { value: true, endPos: pos + 4 };
  }
  if (text.startsWith("false", pos)) {
    return { value: false, endPos: pos + 5 };
  }
  if (text.startsWith("null", pos)) {
    return { value: null, endPos: pos + 4 };
  }
  if (text.startsWith("undefined", pos)) {
    return { value: null, endPos: pos + 9 };
  }

  // Number (including negative)
  const numMatch = text.slice(pos).match(/^-?\d+(\.\d+)?/);
  if (numMatch) {
    return { value: Number(numMatch[0]), endPos: pos + numMatch[0].length };
  }

  // Array
  if (ch === "[") {
    return extractJSArray(text, pos);
  }

  // Object
  if (ch === "{") {
    return extractJSObject(text, pos);
  }

  // Regular expression literal
  if (ch === "/") {
    let i = pos + 1;
    let pattern = "";
    while (i < text.length && text[i] !== "/") {
      if (text[i] === "\\") {
        pattern += text[i] + text[i + 1];
        i += 2;
        continue;
      }
      pattern += text[i];
      i++;
    }
    i++; // skip closing /
    let flags = "";
    while (i < text.length && /[gimsuy]/.test(text[i])) {
      flags += text[i];
      i++;
    }
    return { value: `/${pattern}/${flags}`, endPos: i };
  }

  return null;
}

function extractJSArray(text, pos) {
  if (text[pos] !== "[") return null;

  const items = [];
  let i = pos + 1;

  while (i < text.length) {
    // Skip whitespace and commas
    while (i < text.length && /[\s,]/.test(text[i])) i++;

    if (i >= text.length) return null;
    if (text[i] === "]") {
      return { value: items, endPos: i + 1 };
    }

    // Skip trailing comma scenarios
    if (text[i] === ",") { i++; continue; }

    const item = extractJSValue(text, i);
    if (!item) {
      // Can't parse this item — try to skip to end of array
      const end = findArrayEnd(text, pos);
      if (end >= 0) return { value: items, endPos: end + 1 };
      return null;
    }

    items.push(item.value);
    i = item.endPos;
  }

  return null;
}

function findArrayEnd(text, startPos) {
  let depth = 0;
  let i = startPos;
  while (i < text.length) {
    if (text[i] === "[") depth++;
    else if (text[i] === "]") { depth--; if (depth === 0) return i; }
    else if (text[i] === "'" || text[i] === '"' || text[i] === "`") {
      const q = text[i];
      i++;
      while (i < text.length && text[i] !== q) {
        if (text[i] === "\\") i++;
        i++;
      }
    }
    i++;
  }
  return -1;
}

function extractJSObject(text, pos) {
  if (text[pos] !== "{") return null;

  const obj = {};
  let i = pos + 1;

  while (i < text.length) {
    // Skip whitespace and commas
    while (i < text.length && /[\s,]/.test(text[i])) i++;

    if (i >= text.length) return null;
    if (text[i] === "}") {
      return { value: obj, endPos: i + 1 };
    }

    // Parse key
    let key;
    if (text[i] === "'" || text[i] === '"') {
      const kv = extractJSValue(text, i);
      if (!kv) return null;
      key = kv.value;
      i = kv.endPos;
    } else {
      // Unquoted key (identifier)
      const keyMatch = text.slice(i).match(/^[a-zA-Z_$][a-zA-Z0-9_$]*/);
      if (!keyMatch) {
        // Try to find closing brace
        const end = findMatchingBrace(text, pos);
        if (end >= 0) return { value: obj, endPos: end + 1 };
        return null;
      }
      key = keyMatch[0];
      i += key.length;
    }

    // Skip colon
    while (i < text.length && /\s/.test(text[i])) i++;
    if (i < text.length && text[i] === ":") i++;
    while (i < text.length && /\s/.test(text[i])) i++;

    // Parse value
    const val = extractJSValue(text, i);
    if (!val) {
      // Skip this entry, try to find next comma or closing brace
      while (i < text.length && text[i] !== "," && text[i] !== "}") {
        if (text[i] === "{") {
          const end = findMatchingBrace(text, i);
          if (end >= 0) { i = end + 1; continue; }
        }
        if (text[i] === "[") {
          const end = findArrayEnd(text, i);
          if (end >= 0) { i = end + 1; continue; }
        }
        i++;
      }
      continue;
    }

    obj[key] = val.value;
    i = val.endPos;
  }

  return null;
}

// ---------------------------------------------------------------------------
// testRule block extraction
// ---------------------------------------------------------------------------

/**
 * Determine syntax from customSyntax value or file content.
 * Returns "css", "scss", "less", or null (skip).
 */
function determineSyntax(customSyntax) {
  if (!customSyntax) return "css";

  const s = String(customSyntax).toLowerCase();
  if (s.includes("scss") || s.includes("sass")) return "scss";
  if (s.includes("less")) return "less";
  if (s.includes("html") || s.includes("jsx") || s.includes("css-in-js")) return null; // skip
  if (s.includes("sugarss") || s.includes("sss")) return null; // skip

  return "css";
}

/**
 * Extract a simple property value from a testRule block text.
 * Looks for `propertyName: value` pattern.
 */
function extractProperty(blockText, propName) {
  // Match property: value at the block level (not nested in accept/reject arrays)
  // We need to be careful not to match inside code strings
  const patterns = [
    new RegExp(`(?:^|[\\n,{])\\s*${propName}\\s*:\\s*`, "m"),
  ];

  for (const re of patterns) {
    const match = re.exec(blockText);
    if (!match) continue;

    const valueStart = match.index + match[0].length;
    const result = extractJSValue(blockText, valueStart);
    if (result) return result.value;
  }

  return undefined;
}

/**
 * Extract accept or reject array from a testRule block.
 * Each entry has: { code, description?, line?, column?, endLine?, endColumn?, message?, warnings? }
 */
function extractCaseArray(blockText, arrayName) {
  // Find `accept: [` or `reject: [`
  const re = new RegExp(`(?:^|[\\n,{])\\s*${arrayName}\\s*:\\s*\\[`, "m");
  const match = re.exec(blockText);
  if (!match) return [];

  const arrStart = match.index + match[0].length - 1; // point at [
  const parsed = extractJSArray(blockText, arrStart);
  if (!parsed || !Array.isArray(parsed.value)) return [];

  return parsed.value
    .filter((item) => item && typeof item === "object" && item.code != null)
    .map((item) => {
      const entry = { code: String(item.code) };
      if (item.description) entry.description = String(item.description);
      if (item.line != null) entry.line = Number(item.line);
      if (item.column != null) entry.column = Number(item.column);
      if (item.endLine != null) entry.endLine = Number(item.endLine);
      if (item.endColumn != null) entry.endColumn = Number(item.endColumn);
      if (item.message) entry.message = String(item.message);
      // Some reject entries have a `warnings` array instead of direct line/column
      if (Array.isArray(item.warnings)) {
        entry.warnings = item.warnings;
      }
      return entry;
    });
}

/**
 * Find and extract all testRule() blocks from a file's text content.
 */
function extractTestRuleBlocks(fileText, filePath, ruleName, sourceName) {
  const groups = [];

  // Find all testRule({ ... }) calls
  // Patterns:
  //   testRule({
  //   testRule(\n{
  const re = /testRule\s*\(\s*\{/g;
  let match;

  while ((match = re.exec(fileText)) !== null) {
    const braceStart = fileText.lastIndexOf("{", match.index + match[0].length);
    const braceEnd = findMatchingBrace(fileText, braceStart);

    if (braceEnd < 0) {
      if (VERBOSE) console.log(`    [warn] Could not find matching brace at ${filePath}:${match.index}`);
      continue;
    }

    const blockText = fileText.slice(braceStart, braceEnd + 1);

    // Extract ruleName from block (may override directory-based name)
    let blockRuleName = extractProperty(blockText, "ruleName");
    if (!blockRuleName) {
      // Try to find ruleName defined as a variable in the file
      const varMatch = fileText.match(/const\s+ruleName\s*=\s*['"`]([^'"`]+)['"`]/);
      if (varMatch) {
        blockRuleName = varMatch[1];
      }
    }
    // Use the block's ruleName if it's a string literal, otherwise fall back
    if (typeof blockRuleName === "string" && blockRuleName.length > 0) {
      // If the block ruleName doesn't have a prefix but the source expects one, don't add it
      // (the ruleName in the block is the canonical name)
    } else {
      blockRuleName = ruleName;
    }

    if (!blockRuleName) {
      if (VERBOSE) console.log(`    [warn] No ruleName found in block at ${filePath}`);
      continue;
    }

    // Extract config value
    let config = extractProperty(blockText, "config");
    if (config === undefined) {
      // Default to true if no config specified
      config = true;
    }

    // Extract customSyntax
    const customSyntax = extractProperty(blockText, "customSyntax");
    const syntax = determineSyntax(customSyntax);
    if (syntax === null) {
      if (VERBOSE) console.log(`    [skip] Unsupported customSyntax: ${customSyntax}`);
      continue;
    }

    // Extract accept and reject arrays
    const acceptCases = extractCaseArray(blockText, "accept");
    const rejectCases = extractCaseArray(blockText, "reject");

    if (acceptCases.length === 0 && rejectCases.length === 0) {
      if (VERBOSE) console.log(`    [skip] No accept/reject cases found in block`);
      continue;
    }

    // Build cases array
    const cases = [];
    for (const c of acceptCases) {
      cases.push({ type: "accept", code: c.code, ...(c.description && { description: c.description }) });
    }
    for (const c of rejectCases) {
      const entry = { type: "reject", code: c.code };
      if (c.description) entry.description = c.description;
      if (c.line != null) entry.line = c.line;
      if (c.column != null) entry.column = c.column;
      if (c.endLine != null) entry.endLine = c.endLine;
      if (c.endColumn != null) entry.endColumn = c.endColumn;
      if (c.message) entry.message = c.message;
      cases.push(entry);
    }

    groups.push({
      source: sourceName,
      rule: blockRuleName,
      config,
      syntax,
      cases,
    });
  }

  return groups;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main() {
  console.log("Stylelint Compatibility Test Extractor");
  console.log("======================================\n");

  mkdirSync(CLONES_DIR, { recursive: true });

  const allGroups = [];

  for (const repoConfig of REPOS) {
    console.log(`\nProcessing: ${repoConfig.name}`);
    console.log("-".repeat(50));

    // Clone
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

    // Find test files
    const testFiles = findTestFiles(cloneDir, repoConfig.testGlob);
    console.log(`  [files] Found ${testFiles.length} test files`);

    if (testFiles.length === 0) {
      console.log(`  [warn] No test files found matching: ${repoConfig.testGlob}`);
      continue;
    }

    // Extract test cases from each file
    let totalGroups = 0;
    let totalCases = 0;

    for (const testFile of testFiles) {
      const ruleName = extractRuleNameFromPath(testFile, cloneDir, repoConfig);

      let fileText;
      try {
        fileText = readFileSync(testFile, "utf-8");
      } catch (e) {
        if (VERBOSE) console.log(`    [error] Could not read: ${testFile}`);
        continue;
      }

      const groups = extractTestRuleBlocks(fileText, testFile, ruleName, repoConfig.name);

      for (const group of groups) {
        allGroups.push(group);
        totalGroups++;
        totalCases += group.cases.length;
      }

      if (VERBOSE && groups.length > 0) {
        const rel = relative(cloneDir, testFile);
        console.log(`    ${rel}: ${groups.length} groups, ${groups.reduce((s, g) => s + g.cases.length, 0)} cases`);
      }
    }

    console.log(`  [extracted] ${totalGroups} test groups, ${totalCases} test cases`);
  }

  // Deduplicate: merge groups with same (source, rule, config, syntax)
  // Actually, different testRule blocks for the same rule often have different configs,
  // so keep them separate.

  // Write output
  writeFileSync(OUTPUT_FILE, JSON.stringify(allGroups, null, 2));

  // Summary
  const ruleSet = new Set(allGroups.map((g) => g.rule));
  const totalCases = allGroups.reduce((s, g) => s + g.cases.length, 0);

  console.log("\n" + "=".repeat(50));
  console.log("Summary");
  console.log("=".repeat(50));
  console.log(`  Total test groups:   ${allGroups.length}`);
  console.log(`  Total test cases:    ${totalCases}`);
  console.log(`  Unique rules:        ${ruleSet.size}`);
  console.log(`  Output:              ${OUTPUT_FILE}`);

  // Per-source breakdown
  for (const repo of REPOS) {
    const sourceGroups = allGroups.filter((g) => g.source === repo.name);
    const sourceCases = sourceGroups.reduce((s, g) => s + g.cases.length, 0);
    const sourceRules = new Set(sourceGroups.map((g) => g.rule));
    console.log(`\n  ${repo.name}:`);
    console.log(`    Groups: ${sourceGroups.length}, Cases: ${sourceCases}, Rules: ${sourceRules.size}`);
  }

  console.log();
}

main();
