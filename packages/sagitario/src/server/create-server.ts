interface CreateServerParams {
  isDev?: boolean;
}

export async function createServer({ isDev }: CreateServerParams = {}) {
  console.log(isDev ? 'dev!' : 'dev off');
}
