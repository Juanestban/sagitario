import { color } from '@astrojs/cli-kit';
import { performance } from 'node:perf_hooks';
import fs, { existsSync, type FSWatcher } from 'node:fs';
import type { IncomingMessage, ServerResponse } from 'http';

import { startContainer } from './container.js';
import { createContainerWithAutomaticRestart } from './restart.js';
import type { SagitarioInlineConfig } from '../../@types/sagitario.js';

export interface DevServer {
  handle: (req: IncomingMessage, res: ServerResponse) => void;
  watcher: FSWatcher;
  stop(): Promise<void>;
}

export async function devServer(config: SagitarioInlineConfig): Promise<DevServer> {
  const devStart = performance.now();
  const restart = await createContainerWithAutomaticRestart({ config, fs });

  // const restart: DevServer = {
  //   watcher: (() => {}) as unknown as FSWatcher,
  //   handle: (req: IncomingMessage, res: ServerResponse) => {},
  //   stop: async () => {},
  // };
  const devServerAddressInfo = await startContainer(restart.container);
  console.log(
    color.blue('[+]'),
    console.table({
      startupTime: performance.now() - devStart,
      resolvedUrls: restart.container.server.resolvedUrls || { local: [], network: [] },
      host: restart.container.config.server?.host ?? 'localhost',
      base: restart.container.config.base,
    }),
  );

  return {
    get watcher() {
      return restart.container.server.watcher;
    },

    handle(req: IncomingMessage, res: ServerResponse) {
      return restart.container.handle(req, res);
    },

    async stop() {
      await restart.container.close();
    },
  };
}
