import { color } from '@astrojs/cli-kit';

console.log('Hi! v2');

type Hook = 'dev' | 'build' | 'preview';
const flags = {};

const commands: Record<Hook, any> = {
  build: async () => {
    const { build } = await import('./build/index.js');
    await build({ flags });
    return;
  },
  dev: async () => {
    const { dev } = await import('./dev/index.js');
    const server = await dev({ flags });
    if (server) {
      return await new Promise(() => {}); // forever love!!! ❤️‍🔥
    }

    return;
  },
  preview: async () => {
    const { preview } = await import('./preview/index.js');
    const server = await preview({ flags });

    if (server) {
      return await server.closed();
    }

    return;
  },
};

function startCommand(cmd: string) {
  if (!commands.hasOwnProperty(cmd)) {
    console.log(color.red("This command don't exist."));
    throw new Error(`error running ${cmd} -- no command found or supported.`);
  }

  const result = commands[cmd as Hook];
  result();
}

export async function cli(argsv: string[]) {
  try {
    const cmd = argsv[0];
    startCommand(cmd);
  } catch (error) {
    console.log(color.red('[Error]:') + "can't run command - exiting");
    process.exit(1);
  }
}
