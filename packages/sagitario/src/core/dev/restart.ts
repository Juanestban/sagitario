import fs from 'node:fs';
import { color } from '@astrojs/cli-kit';
import type { CLIShortcut } from 'vite';
import { normalizePath } from 'vite';

import type { SagitarioInlineConfig } from '../../@types/sagitario.js';
import { createContainer, startContainer, closeContainer, type Container } from './container.js';

async function createRestartedContainer(
  container: Container,
  config: SagitarioInlineConfig,
): Promise<Container> {
  const { fs } = container;
  const newContainer = await createContainer({
    isRestart: true,
    config,
    fs,
  });

  await startContainer(newContainer);

  return newContainer;
}

function shouldRestartContainer({ config }: Container, changedFile: string): boolean {
  let shouldRestart = false;

  if (!shouldRestart && config.watchFiles.length > 0) {
    shouldRestart = config.watchFiles.some(
      (path) => normalizePath(path) === normalizePath(changedFile),
    );
  }

  return shouldRestart;
}

async function restartContainer(container: Container): Promise<Container | Error> {
  const { close, config: existingSettings } = container;

  try {
    await close();
    return await createRestartedContainer(container, existingSettings);
  } catch (_err) {
    console.error(_err);
    console.error(color.red('[-]'), 'Continuing with previous valid configuration\n');
    return new Error((_err as any).message ?? 'Error to try restart container');
  }
}

interface CreateContainerWithAutomaticRestart {
  config: SagitarioInlineConfig;
  fs: typeof fs;
}

interface Restart {
  container: Container;
  restarted: () => Promise<Error | null>;
}

export async function createContainerWithAutomaticRestart({
  config,
  fs,
}: CreateContainerWithAutomaticRestart): Promise<Restart> {
  console.log('config ->', config);
  const container = await createContainer({ config, fs });

  let resolveRestart: (value: Error | null) => void;
  let restartComplete = new Promise<Error | null>((resolve) => {
    resolveRestart = resolve;
  });

  let restart: Restart = {
    container,
    restarted() {
      return restartComplete;
    },
  };

  async function handleServerRestart(logMsg = '') {
    console.log(color.blue(logMsg + ' Restarting...').trim());
    const container = restart.container;
    const result = await restartContainer(container);
    if (result instanceof Error) {
      resolveRestart(result);
    } else {
      restart.container = result;
      // setupContainer();
      resolveRestart(null);
    }
    restartComplete = new Promise<Error | null>((resolve) => {
      resolveRestart = resolve;
    });
  }

  function handleChangeRestart(logMsg: string) {
    return async function (changedFile: string) {
      if (shouldRestartContainer(restart.container, changedFile)) {
        handleServerRestart(logMsg);
      }
    };
  }

  function setupContainer() {
    const watcher = restart.container.server.watcher;
    watcher.on('change', handleChangeRestart('Configuration file updated.'));
    watcher.on('unlink', handleChangeRestart('Configuration file removed.'));
    watcher.on('add', handleChangeRestart('Configuration file added.'));

    // Restart the Astro dev server instead of Vite's when the API is called by plugins.
    // Ignore the `forceOptimize` parameter for now.
    restart.container.server.restart = () => handleServerRestart();

    // Set up shortcuts

    const customShortcuts: Array<CLIShortcut> = [
      // Disable default Vite shortcuts that don't work well with Astro
      { key: 'r', description: '' },
      { key: 'u', description: '' },
      { key: 'c', description: '' },
    ];

    restart.container.server.bindCLIShortcuts({
      customShortcuts,
    });
  }
  setupContainer();

  return restart;
}
