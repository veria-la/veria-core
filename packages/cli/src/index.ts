#!/usr/bin/env node
import { Command } from 'commander';
import kleur from 'kleur';
import { registerFold } from './commands/fold.js';
import { registerVerify } from './commands/verify.js';
import { registerDeploy } from './commands/deploy.js';
import { registerCircuits } from './commands/circuits.js';
import { registerCost } from './commands/cost.js';

const program = new Command();

program
  .name('veria')
  .description('Solana-native ZK Coprocessor CLI. Few dots. Whole truth.')
  .version('0.1.0')
  .option('--api-url <url>', 'Compute API base URL', process.env.VERIA_API_URL)
  .option(
    '--program <pubkey>',
    'Anchor verifier program ID',
    process.env.VERIA_PROGRAM_ID,
  )
  .option('--cluster <cluster>', 'Solana cluster', 'devnet');

registerFold(program);
registerVerify(program);
registerDeploy(program);
registerCircuits(program);
registerCost(program);

program.parseAsync(process.argv).catch((err: Error) => {
  process.stderr.write(kleur.red(`\nveria: ${err.message}\n`));
  if (process.env.VERIA_DEBUG) {
    process.stderr.write(`${err.stack}\n`);
  }
  process.exit(1);
});
