import { color } from '@astrojs/cli-kit';
import { devServer } from '../../core/dev/index.js';

const { cyan } = color;

interface DevParams {
  flags?: unknown;
}

const flagsToSagitarioConfig = ({ flags }: DevParams): Record<string, any> => {
  return {};
};

export async function dev({ flags }: DevParams) {
  const inlineConfig = flagsToSagitarioConfig({ flags });
  return await devServer(inlineConfig);
}
