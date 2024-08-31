import fs from 'node:fs/promises';
import esbuild from 'esbuild';
//import { copy } from 'esbuild-plugin-copy';
import { color } from '@astrojs/cli-kit';
import glob from 'fast-glob';

import prebuild from './prebuild.js';

const { green, red, yellow, dim } = color;

/** @type {import('esbuild').BuildOptions} */
const defaultConfig = {
  minify: false,
  format: 'esm',
  platform: 'node',
  target: 'node18',
  sourcemap: false,
  sourcesContent: false,
};

const dt = new Intl.DateTimeFormat('en-us', {
  hour: '2-digit',
  minute: '2-digit',
});

function getPrebuilds(isDev, args) {
  let prebuilds = [];
  while (args.includes('--prebuild')) {
    let idx = args.indexOf('--prebuild');
    prebuilds.push(args[idx + 1]);
    args.splice(idx, 2);
  }
  if (prebuilds.length && isDev) {
    prebuilds.unshift('--no-minify');
  }
  return prebuilds;
}

export default async function build(...args) {
  const config = Object.assign({}, defaultConfig);
  const isDev = args.slice(-1)[0] === 'IS_DEV';
  const prebuilds = getPrebuilds(isDev, args);
  const patterns = args
    .filter((f) => !!f) // remove empty args
    .map((f) => f.replace(/^'/, '').replace(/'$/, '')); // Needed for Windows: glob strings contain surrounding string chars??? remove these
  let entryPoints = [].concat(
    ...(await Promise.all(
      patterns.map((pattern) => glob(pattern, { filesOnly: true, absolute: true })),
    )),
  );

  const noClean = args.includes('--no-clean-dist');
  const bundle = args.includes('--bundle');
  const forceCJS = args.includes('--force-cjs');
  //const copyWASM = args.includes('--copy-wasm');

  const { type = 'module', dependencies = {} } = await readPackageJSON('./package.json');

  config.define = {};
  for (const [key, value] of await getDefinedEntries()) {
    config.define[`process.env.${key}`] = JSON.stringify(value);
  }
  const format = type === 'module' && !forceCJS ? 'esm' : 'cjs';

  const outdir = 'dist';

  if (!noClean) {
    await clean(outdir);
  }

  if (!isDev) {
    await esbuild.build({
      ...config,
      bundle,
      external: bundle ? Object.keys(dependencies) : undefined,
      entryPoints,
      outdir,
      outExtension: forceCJS ? { '.js': '.cjs' } : {},
      format,
      // plugins: [copyPlugin].filter(Boolean),
    });
    return;
  }

  const rebuildPlugin = {
    name: 'sagitario:rebuild',
    setup(build) {
      build.onEnd(async (result) => {
        if (prebuilds.length) {
          await prebuild(...prebuilds);
        }

        const date = dt.format(new Date());

        if (result && result.errors.length) {
          console.error(dim(`[${date}] `) + red(error || result.errors.join('\n')));
        } else {
          if (result.warnings.length) {
            console.log(
              dim(`[${date}] `) +
                yellow('⚠ updated with warnings:\n' + result.warnings.join('\n')),
            );
          }

          console.log(dim(`[${date}] `) + green('✔  updated'));
        }
      });
    },
  };
  const builder = await esbuild.context({
    ...config,
    entryPoints,
    outdir,
    format,
    sourcemap: 'linked',
    plugins: [rebuildPlugin /** copyPlugin */].filter(Boolean),
  });

  await builder.watch();

  process.on('beforeExit', () => {
    builder.stop && builder.stop();
  });
}

async function clean(outdir) {
  const files = await glob([`${outdir}/**`, `!${outdir}/**/*.d.ts`], { filesOnly: true });
  await Promise.all(files.map((file) => fs.rm(file, { force: true })));
}

async function getDefinedEntries() {
  const define = {
    PACKAGE_VERSION: await getInternalPackageVersion('./package.json'),
    SAGITARIO_VERSION: await getInternalPackageVersion(
      new URL('../../packages/sagitario/package.json', import.meta.url),
    ),
    //SAGITARIO_CHECK_VERSION: await getWorkspacePackageVersion('@astrojs/check'),
    TYPESCRIPT_VERSION: await getWorkspacePackageVersion('typescript'),
  };
  for (const [key, value] of Object.entries(define)) {
    if (value === undefined) {
      delete define[key];
    }
  }
  return Object.entries(define);
}

async function readPackageJSON(path) {
  return await fs.readFile(path, { encoding: 'utf8' }).then((res) => JSON.parse(res));
}

async function getInternalPackageVersion(path) {
  return readPackageJSON(path).then((res) => res.version);
}

async function getWorkspacePackageVersion(packageName) {
  const { dependencies, devDependencies } = await readPackageJSON(
    new URL('../../package.json', import.meta.url),
  );
  const deps = { ...dependencies, ...devDependencies };
  const version = deps[packageName];
  if (!version) {
    throw new Error(
      `Unable to resolve "${packageName}". Is it a dependency of the workspace root?`,
    );
  }
  return version.replace(/^\D+/, '');
}
