import { type ChildProcess, fork } from 'node:child_process';
import path from 'node:path';

import type { SagitarioConfig } from '../@types/sagitario.js';
import { devServer } from '../core/index.js';

interface DevParams {
  flags?: unknown;
}

let child: ChildProcess | undefined;
const initialEnv = { NODE_ENV: 'development' };

const configSagitarioConfig = (): SagitarioConfig => {
  return { hostname: 'localhost', port: 3000, watchFiles: [], root: process.cwd() };
};

export async function dev({}: DevParams) {
  const sagitarioConfig = configSagitarioConfig();
  const serverPath = path.resolve(import.meta.dirname, '../core/index.ts');
  const defaultEnv = (initialEnv || process.env) as typeof process.env;

  return await devServer(sagitarioConfig);
}
