// The client of the Sagitario language server: starts `sag lsp` for `.sag` files
// (docs/adr/0058-language-server.md). Highlighting, snippets and the file icon work without it.
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

/** @type {LanguageClient | undefined} */
let client;

/**
 * The command of the server (docs/adr/0071-vscode-extension-in-the-marketplace.md): the setting
 * `sagitario.server.path`, else the `sag` of the installers in `~/.sagitario/bin`, else `sag` on
 * PATH, which an editor started before installing, or from the Dock on macOS, may not see.
 */
function serverCommand() {
  const configured = vscode.workspace.getConfiguration('sagitario').get('server.path');

  if (typeof configured === 'string' && configured.trim() !== '') {
    return configured.trim();
  }

  const installed = path.join(os.homedir(), '.sagitario', 'bin', process.platform === 'win32' ? 'sag.exe' : 'sag');

  return fs.existsSync(installed) ? installed : 'sag';
}

/** The command that installs Sagitario, as the README gives it. */
function installCommand() {
  return process.platform === 'win32'
    ? 'irm https://github.com/Juanestban/sagitario/releases/latest/download/install.ps1 | iex'
    : 'curl -fsSL https://github.com/Juanestban/sagitario/releases/latest/download/install.sh | sh';
}

async function start(context) {
  const command = serverCommand();
  const server = { command, args: ['lsp'] };

  client = new LanguageClient(
    'sagitario',
    'Sagitario',
    { run: server, debug: server },
    { documentSelector: [{ scheme: 'file', language: 'sagitario' }] },
  );

  try {
    await client.start();
  } catch (error) {
    client = undefined;

    const action = await vscode.window.showWarningMessage(
      `Sagitario: cannot start the language server \`${command} lsp\` (${error.message ?? error}). ` +
        `Install Sagitario (\`${installCommand()}\`) or set \`sagitario.server.path\` to a \`sag\`. ` +
        'Highlighting and snippets keep working without it.',
      'Open settings',
    );

    if (action === 'Open settings') {
      vscode.commands.executeCommand('workbench.action.openSettings', 'sagitario.server.path');
    }
  }

  context.subscriptions.push({ dispose: () => client?.stop() });
}

async function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand('sagitario.restartServer', async () => {
      await client?.stop();
      await start(context);
    }),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration('sagitario.server.path')) {
        await client?.stop();
        await start(context);
      }
    }),
  );

  await start(context);
}

async function deactivate() {
  await client?.stop();
}

module.exports = { activate, deactivate };
