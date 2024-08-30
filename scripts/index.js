#!/usr/bin/env node
export default async function run() {
  const commands = {
    dev: async () => {
      const { default: build } = await import('./cmd/build.js');

      await build(...args, 'IS_DEV');
    },
    build: async () => {
      const { default: build } = await import('./cmd/build.js');

      await build(...args, undefined);
    },
    prebuild: async () => {
      const { default: prebuild } = await import('./cmd/prebuild.js');

      await prebuild(...args);
    },
    test: async () => {
      const { default: test } = await import('./cmd/test.js');

      await test(...args);
    },
  };

  const [cmd, ...args] = process.argv.slice(2);

  commands.hasOwnProperty(cmd) ? commands[cmd]() : console.log('command not found');
}

run();
