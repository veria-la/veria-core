import { readFileSync, writeFileSync } from 'node:fs';
import { Command } from 'commander';
import kleur from 'kleur';
import ora from 'ora';
import { CIRCUITS, VeriaClient, type CircuitId } from '@veria/sdk';
import { getGlobalOpts } from '../config.js';

interface FoldOpts {
  circuit: CircuitId;
  subProofs: string;
  output?: string;
}

export function registerFold(root: Command): void {
  root
    .command('fold')
    .description('Submit a folded ZK computation job to the VERIA Compute API')
    .argument('<input>', 'Path to input JSON file')
    .requiredOption(
      '-c, --circuit <id>',
      'Circuit id (scoring|aggregation|median|sort|ml-inference)',
    )
    .option('-n, --sub-proofs <n>', 'Number of sub-proofs to fold', '100')
    .option('-o, --output <path>', 'Write proof bytes to this file')
    .action(async (inputPath: string, opts: FoldOpts, cmd: Command) => {
      if (!(opts.circuit in CIRCUITS)) {
        process.stderr.write(
          kleur.red(
            `Unknown circuit "${opts.circuit}". Known: ${Object.keys(CIRCUITS).join(', ')}\n`,
          ),
        );
        process.exit(1);
      }

      const subProofCount = Number.parseInt(opts.subProofs, 10);
      if (!Number.isFinite(subProofCount) || subProofCount < 1) {
        process.stderr.write(kleur.red('--sub-proofs must be a positive integer\n'));
        process.exit(1);
      }

      const raw = readFileSync(inputPath, 'utf-8');
      const input = JSON.parse(raw) as unknown;

      const globalOpts = getGlobalOpts(cmd);
      const client = new VeriaClient({
        apiUrl: globalOpts.apiUrl,
        programId: globalOpts.program,
      });

      const cost = client.computeCost(subProofCount);
      process.stderr.write(
        `${kleur.gray('circuit')}     ${kleur.bold(opts.circuit)}\n` +
          `${kleur.gray('sub-proofs')}  ${subProofCount}\n` +
          `${kleur.gray('direct cost')} ${cost.directTotalSol.toFixed(4)} SOL\n` +
          `${kleur.gray('folded cost')} ${cost.foldedSol.toFixed(4)} SOL\n` +
          `${kleur.gray('savings')}     ${cost.savingsPct.toFixed(2)}%\n\n`,
      );

      const spinner = ora({ text: 'Folding via Nova IVC...', stream: process.stderr }).start();
      try {
        const res = await client.fold({ circuit: opts.circuit, input, subProofCount });
        spinner.succeed(`Proof ready (jobId=${res.jobId}, ${res.proofBytes.length}B)`);

        const hexProof = toHex(res.proofBytes);
        const hexPub = toHex(res.publicInputs);
        const localHash = client.localProofHash(res.proofBytes, res.publicInputs);

        process.stdout.write(
          JSON.stringify(
            {
              jobId: res.jobId,
              circuit: res.circuit,
              subProofCount: res.subProofCount,
              proofBytesHex: hexProof,
              publicInputsHex: hexPub,
              proofHash: localHash,
              costSol: res.costSol,
              directCostSol: res.directCostSol,
              savingsPct: res.savingsPct,
            },
            null,
            2,
          ) + '\n',
        );

        if (opts.output) {
          writeFileSync(opts.output, res.proofBytes);
          process.stderr.write(kleur.green(`Wrote ${opts.output}\n`));
        }
      } catch (exc) {
        spinner.fail((exc as Error).message);
        process.exit(1);
      }
    });
}

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}
