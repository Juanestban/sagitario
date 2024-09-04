export interface SagitarioConfig {
  hostname: string;
  port: number;
  watchFiles: string[];
  root: string;
  keepAliveTimeout?: number;
}
