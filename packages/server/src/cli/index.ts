import { color } from '@astrojs/cli-kit';

const { red, blue, white, green, bgBlue } = color;

enum Command {
  DEV = 'dev',
  START = 'start',
  BUILD = 'build',
}

async function startCommand(cmd: Command, args: string[]) {
  const [] = process.argv.slice(2);

  switch (cmd) {
    case Command.BUILD: {
      console.log(bgBlue(white('Working on that ') + '🚧'));
      return;
    }
    case Command.START: {
      console.log(bgBlue(white('Working on that ') + '🚧'));
      return;
    }
    case Command.DEV: {
      const { dev } = await import('./dev.js');
      const server = await dev({ flags: args });

      if (server) {
        return await new Promise(() => {});
      }
      return;
    }
  }

  throw new Error(`Error running ${cmd} -- no command found.`);
}

export async function cli(argsv: string[]) {
  const [cmd, ...args] = argsv.slice(2);

  try {
    await startCommand(cmd as unknown as Command, args);
  } catch (error) {
    console.log(red('[Error]:'), "can't run command - exiting");
    console.error(error);
    process.exit(1);
  }
}
