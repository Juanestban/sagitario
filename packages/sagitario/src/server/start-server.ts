import { createServer } from './create-server.js';

export async function startServer() {
  try {
    await createServer();
    console.log('done');
  } catch (error) {
    console.error(error);
  } finally {
    console.log('finally');
  }
}
