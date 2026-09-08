"use strict";

/**
 * Maps a Node.js platform and architecture to the Rust target whose binary
 * ships inside this package, and names the binary for that platform.
 *
 * Shared by the `gale` launcher and the programmatic API. CommonJS so the
 * launcher can require it synchronously on every supported Node version.
 */

const TARGETS = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "linux-arm64": "aarch64-unknown-linux-gnu",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "win32-arm64": "aarch64-pc-windows-msvc",
  "win32-x64": "x86_64-pc-windows-msvc",
};

/**
 * The Rust target triple for a platform/arch pair, or `null` when Gale does
 * not ship a binary for it.
 *
 * @param {string} platform - `process.platform`
 * @param {string} arch - `process.arch`
 * @returns {string | null}
 */
function resolveTarget(platform, arch) {
  return TARGETS[`${platform}-${arch}`] ?? null;
}

/**
 * The file name of the Gale binary on a platform.
 *
 * @param {string} platform - `process.platform`
 * @returns {string}
 */
function binaryFileName(platform) {
  return platform === "win32" ? "gale.exe" : "gale";
}

/** Every `platform-arch` pair a binary ships for, for error messages. */
function supportedPlatforms() {
  return Object.keys(TARGETS);
}

exports.resolveTarget = resolveTarget;
exports.binaryFileName = binaryFileName;
exports.supportedPlatforms = supportedPlatforms;
