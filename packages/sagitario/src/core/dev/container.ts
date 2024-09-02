import nodeFs from 'node:fs';
import { color } from '@astrojs/cli-kit';
import type { AddressInfo } from 'node:net';
import { createServer, type ViteDevServer } from 'vite';
import { IncomingMessage, ServerResponse } from 'node:http';

import type { SagitarioInlineConfig } from '../../@types/sagitario.js';

export interface Container {
  fs: typeof nodeFs;
  config: SagitarioInlineConfig;
  server: ViteDevServer;
  handle: (req: IncomingMessage, res: ServerResponse) => void;
  close: () => Promise<void>;
}

export interface CreateContainerParams {
  config: SagitarioInlineConfig;
  isRestart?: boolean;
  fs?: typeof nodeFs;
}

export async function createContainer({ isRestart, config, fs = nodeFs }: CreateContainerParams) {
  const viteServer = await createServer(config);
  const container: Container = {
    fs,
    config,
    server: viteServer,
    handle(req, res) {
      viteServer.middlewares.handle(req, res, Function.prototype);
    },
    close() {
      return closeContainer(container);
    },
  };

  return container;
}

export async function startContainer({ config, server }: Container): Promise<AddressInfo> {
  const { port } = config;
  await server.listen(port);
  const devServerAddressInfo = server.httpServer!.address() as AddressInfo;
  console.log(color.blue('[-]'), 'starting... ⏳');

  return devServerAddressInfo;
}

export async function closeContainer({ server }: Container) {
  await server.close();
  console.log(color.gray('[-]'), 'closing server... 🌑');
}
