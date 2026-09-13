#!/usr/bin/env node
"use strict";

/**
 * Launches the bundled Gale binary for this platform.
 *
 * A Node script rather than a shell script so the same launcher works on
 * Windows, where npm wraps it in a .cmd shim. Everything is forwarded: the
 * arguments, stdin/stdout/stderr, the exit code, and any terminating signal.
 */

const { spawnSync } = require("node:child_process");
const { existsSync } = require("node:fs");
const path = require("node:path");

const { binaryFileName, resolveTarget, supportedPlatforms } = require("../platform.cjs");

const target = resolveTarget(process.platform, process.arch);

if (!target) {
  process.stderr.write(`Unsupported platform: ${process.platform}-${process.arch}\n`);
  process.stderr.write(`Supported: ${supportedPlatforms().join(", ")}\n`);
  process.stderr.write("Build from source instead: cargo install gale-lint\n");
  process.exit(1);
}

const binary = path.join(__dirname, target, binaryFileName(process.platform));

if (!existsSync(binary)) {
  process.stderr.write(`Gale binary missing for ${target} at ${binary}\n`);
  process.stderr.write(`This package should ship prebuilt binaries for ${target}.\n`);
  process.stderr.write("Try reinstalling the package, or build from source:\n");
  process.stderr.write("  cargo install gale-lint\n");
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), {
  stdio: "inherit",
  windowsHide: true,
});

if (result.error) {
  process.stderr.write(`Failed to run ${binary}: ${result.error.message}\n`);
  process.exit(1);
}

if (result.signal) {
  // Re-raise so the parent sees the same signal the linter died from.
  process.kill(process.pid, result.signal);
}

process.exit(result.status ?? 1);
