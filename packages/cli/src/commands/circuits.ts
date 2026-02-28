import { Command } from 'commander';
import kleur from 'kleur';
import { listCircuits } from '@veria/sdk';

export function registerCircuits(root: Command): void {
  root
    .command('circuits')
    .description('List built-in circuits')
    .option('--json', 'Emit JSON instead of a table')
    .action((opts: { json?: boolean }) => {
      const list = listCircuits();
      if (opts.json) {
        process.stdout.write(JSON.stringify(list, null, 2) + '\n');
        return;
      }
      process.stdout.write(`${kleur.bold('ID  Name             Max input  Tests  Description')}\n`);
      for (const c of list) {
        const id = c.numericId.toString().padEnd(3);
        const name = c.name.padEnd(16);
        const max = c.maxInputSize.toString().padStart(9);
        const tests = c.testCount.toString().padStart(5);
        process.stdout.write(
          `${id} ${name} ${max}  ${tests}  ${c.description}\n`,
        );
      }
    });
}
