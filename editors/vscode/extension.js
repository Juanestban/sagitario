// The client of the Sagitario language server: starts `sag lsp` for `.sag` files
// (docs/adr/0058-language-server.md). Highlighting, snippets and the file icon work without it.
'use strict';

const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

/** @type {LanguageClient | undefined} */
let client;

/** The command of the server: the setting `sagitario.server.path`, or `sag` on PATH. */
function serverCommand() {
  const configured = vscode.workspace.getConfiguration('sagitario').get('server.path');

  return typeof configured === 'string' && configured.trim() !== '' ? configured.trim() : 'sag';
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
        'Install `sag` (`bootstrap/build.sh --install`, or `bootstrap/build.ps1 --install` on Windows) or set `sagitario.server.path` to it. ' +
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
