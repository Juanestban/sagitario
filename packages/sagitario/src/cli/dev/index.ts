import { color } from '@astrojs/cli-kit';

import { devServer, type DevServer } from '../../core/dev/index.js';
import type { SagitarioInlineConfig } from '../../@types/sagitario.js';

const { cyan } = color;

interface DevParams {
  flags?: unknown;
}

const flagsToSagitarioConfig = ({ flags }: DevParams): SagitarioInlineConfig => {
  return { port: 3000, watchFiles: [], root: process.cwd() };
};

export async function dev({ flags }: DevParams): Promise<DevServer> {
  const inlineConfig = flagsToSagitarioConfig({ flags });

  return await devServer(inlineConfig);
}
