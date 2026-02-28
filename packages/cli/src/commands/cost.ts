import { Command } from 'commander';
import kleur from 'kleur';
import { VeriaClient } from '@veria/sdk';

export function registerCost(root: Command): void {
  root
    .command('cost')
    .description('Show direct vs folded SOL cost for N sub-proofs')
    .requiredOption('-n, --sub-proofs <n>', 'Number of sub-proofs')
    .option('--json', 'Emit JSON')
    .action((opts: { subProofs: string; json?: boolean }) => {
      const n = Number.parseInt(opts.subProofs, 10);
      if (!Number.isFinite(n) || n < 1) {
        process.stderr.write(kleur.red('--sub-proofs must be positive integer\n'));
        process.exit(1);
      }
      const bd = new VeriaClient().computeCost(n);
      if (opts.json) {
        process.stdout.write(JSON.stringify(bd, null, 2) + '\n');
        return;
      }
      process.stdout.write(
        `${kleur.gray('sub-proofs')}    ${bd.subProofs}\n` +
          `${kleur.gray('direct/proof')} ${bd.directSolPerSubProof.toFixed(4)} SOL\n` +
          `${kleur.gray('direct total')} ${bd.directTotalSol.toFixed(4)} SOL\n` +
          `${kleur.gray('folded')}       ${bd.foldedSol.toFixed(4)} SOL\n` +
          `${kleur.gray('savings')}      ${bd.savingsPct.toFixed(2)}%\n`,
      );
    });
}
