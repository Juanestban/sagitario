import type { InlineConfig } from 'vite';

export interface SagitarioInlineConfig extends InlineConfig {
  root: string;
  port: number;
  watchFiles: string[];
}
