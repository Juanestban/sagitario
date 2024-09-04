import http, { type IncomingMessage, type ServerResponse } from 'http';
import type { Duplex } from 'node:stream';

import type { SagitarioConfig } from '../@types/sagitario.js';

export type WorkerRequestHandler = (req: IncomingMessage, res: ServerResponse) => Promise<any>;

export type WorkerUpgradeHandler = (req: IncomingMessage, socket: Duplex, head: Buffer) => any;

export async function devServer(params: SagitarioConfig) {
  const { keepAliveTimeout } = params;
  let { port, root, watchFiles, hostname } = params;
  const server = http.createServer(requestListener);

  let handlersReady = () => {};
  let handlersError = () => {};
  let handlersPromise: Promise<void> | undefined = new Promise<void>((resolve, reject) => {
    handlersReady = resolve;
    handlersError = reject;
  });

  let requestHandler: WorkerRequestHandler = async (
    req: IncomingMessage,
    res: ServerResponse,
  ): Promise<void> => {
    if (handlersPromise) {
      await handlersPromise;
      return requestHandler(req, res);
    }
    throw new Error('Invariant request handler was not setup');
  };

  let upgradeHandler: WorkerUpgradeHandler = async (req, socket, head): Promise<void> => {
    if (handlersPromise) {
      await handlersPromise;
      return upgradeHandler(req, socket, head);
    }
    throw new Error('Invariant upgrade handler was not setup');
  };

  async function requestListener(req: IncomingMessage, res: ServerResponse) {
    try {
      await requestHandler(req, res);
    } catch (_err) {
      res.statusCode = 500;
      res.end('Internal Server Error');
      // logger->Error()
      console.error(_err);
    }
  }

  if (keepAliveTimeout) {
    server.keepAliveTimeout = keepAliveTimeout;
  }

  server.on('upgrade', async (req, soket, head) => {
    try {
      await upgradeHandler(req, soket, head);
    } catch (_err) {
      soket.destroy();
      // Log->Error
      console.error(_err);
    }
  });

  let portRetryCount = 0;

  server.on('error', (err: NodeJS.ErrnoException) => {
    if (err.code === 'EADDRINUSE' && portRetryCount < 10) {
      // Log->Warn (port is used)
      port += 1;
      portRetryCount += 1;
      server.listen(port, hostname);
    } else {
      // Log->Error(Failed to start sever)
      console.error(err);
      process.exit(1);
    }
  });

  return new Promise<void>((resolve) => {
    server.on('listening', async () => {
      const addr = server.address();
      const host = hostname;
      port = typeof addr === 'object' ? addr?.port || port : port;
      const networkUrl = host ? `http://${host}:${port}` : null;
      const appUrl = `http://${host}/:${port}`;

      process.env.PORT = port + '';
      process.env.SAGITARIO_PRIVATE_ORIGIN = appUrl;

      let envInfo: string[] | undefined;
      let expFeatureInfo: string[] | undefined;
      const isDev = true;
      // Log->event(Starting...)

      try {
        //
      } catch (_err) {
        console.error(_err);
        process.exit(1);
      }

      resolve();
    });

    server.listen(port, hostname);
  });
  // server
}
