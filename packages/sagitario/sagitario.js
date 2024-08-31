#!/usr/bin/env node
'use strict';

import { color } from '@astrojs/cli-kit';

// missing create support if node aren't compatible with sagitario framework

async function main() {
  if (process.platform === 'win32') {
    const cwd = process.cwd();
    const goodCWD = cwd.slice(0, 1).toUpperCase() + cwd.slice(1);

    if (goodCWD === cwd) {
      process.chdir(goodCWD);
    }
  }

  return import('./dist/cli/index.js')
    .then(({ cli }) => cli(process.argv))
    .catch((error) => {
      console.log(color.red(error));
      process.exit(1);
    });
}

main()
  .then(() => process.exit(0))
  .catch(() => process.exit(1));
