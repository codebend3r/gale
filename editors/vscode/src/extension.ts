import * as path from "node:path";
import * as fs from "node:fs";
import {
	type ExtensionContext,
	workspace,
	window,
	commands,
} from "vscode";
import {
	LanguageClient,
	type LanguageClientOptions,
	type ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

/**
 * Resolve the path to the `gale` binary.
 *
 * Search order:
 *  1. Explicit path from settings (gale.path)
 *  2. node_modules/.bin/gale relative to the workspace root
 *  3. Bundled binary next to the extension
 *  4. Globally installed `gale` on PATH
 */
function findBinary(workspaceRoot: string | undefined): string | undefined {
	// 1. Explicit override from settings
	const configured = workspace
		.getConfiguration("gale")
		.get<string>("path");
	if (configured) {
		return configured;
	}

	// 2. node_modules/.bin inside workspace
	if (workspaceRoot) {
		const localBin = path.join(workspaceRoot, "node_modules", ".bin", "gale");
		if (fs.existsSync(localBin)) {
			return localBin;
		}
	}

	// 3. Bundled binary next to the extension (for future bundled distributions)
	const bundled = path.join(__dirname, "..", "bin", "gale");
	if (fs.existsSync(bundled)) {
		return bundled;
	}

	// 4. Fall back to PATH — the OS will resolve it
	return "gale";
}

function buildServerArgs(): string[] {
	const args = ["--lsp"];
	const configPath = workspace
		.getConfiguration("gale")
		.get<string>("configPath");
	if (configPath) {
		args.push("--config", configPath);
	}
	return args;
}

async function startClient(): Promise<void> {
	const workspaceRoot = workspace.workspaceFolders?.[0]?.uri.fsPath;
	const binary = findBinary(workspaceRoot);

	if (!binary) {
		window.showErrorMessage(
			"Gale binary not found. Install it with `npm i -D @codebend3r/gale`, set `gale.path`, or make sure `gale` is on your PATH.",
		);
		return;
	}

	const serverOptions: ServerOptions = {
		command: binary,
		args: buildServerArgs(),
	};

	const clientOptions: LanguageClientOptions = {
		documentSelector: [
			{ scheme: "file", language: "css" },
			{ scheme: "file", language: "scss" },
			{ scheme: "file", language: "less" },
			{ scheme: "file", language: "sass" },
		],
		synchronize: {
			fileEvents: workspace.createFileSystemWatcher(
				"**/{gale.json,gale.toml,.stylelintrc,stylelint.config.*,.stylelintrc.*}",
			),
		},
	};

	client = new LanguageClient(
		"gale",
		"Gale CSS Linter",
		serverOptions,
		clientOptions,
	);

	await client.start();
}

async function stopClient(): Promise<void> {
	if (client) {
		await client.stop();
		client = undefined;
	}
}

export async function activate(context: ExtensionContext): Promise<void> {
	const config = workspace.getConfiguration("gale");

	if (config.get<boolean>("enable", true)) {
		await startClient();
	}

	// React to configuration changes
	context.subscriptions.push(
		workspace.onDidChangeConfiguration(async (e) => {
			if (e.affectsConfiguration("gale.enable")) {
				const enabled = workspace
					.getConfiguration("gale")
					.get<boolean>("enable", true);
				if (enabled && !client) {
					await startClient();
				} else if (!enabled && client) {
					await stopClient();
				}
			}

			if (
				e.affectsConfiguration("gale.configPath") ||
				e.affectsConfiguration("gale.path")
			) {
				// Restart the server to pick up the new settings
				if (client) {
					await stopClient();
					await startClient();
				}
			}
		}),
	);

	// Provide a manual restart command
	context.subscriptions.push(
		commands.registerCommand("gale.restart", async () => {
			await stopClient();
			await startClient();
			window.showInformationMessage("Gale LSP restarted.");
		}),
	);
}

export async function deactivate(): Promise<void> {
	await stopClient();
}
