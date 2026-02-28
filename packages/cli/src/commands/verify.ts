import { readFileSync } from 'node:fs';
import { Command } from 'commander';
import kleur from 'kleur';
import ora from 'ora';
import { Connection } from '@solana/web3.js';
import { CIRCUITS, VeriaVerifier, type CircuitId } from '@veria/sdk';
import { clusterRpc, getGlobalOpts } from '../config.js';

interface VerifyOpts {
  circuit: CircuitId;
  proofFile: string;
  publicFile: string;
  submitter?: string;
}

export function registerVerify(root: Command): void {
  root
    .command('verify')
    .description('Verify a folded proof against the Anchor program on the configured cluster')
    .requiredOption('-c, --circuit <id>', 'Circuit id')
    .requiredOption('--proof-file <path>', 'Path to proof bytes (.bin)')
    .requiredOption('--public-file <path>', 'Path to public inputs (.bin)')
    .option('--submitter <pubkey>', 'Submitter pubkey (defaults to keypair)')
    .action(async (opts: VerifyOpts, cmd: Command) => {
      const global = getGlobalOpts(cmd);
      if (!global.program) {
        process.stderr.write(
          kleur.red(
            'Missing --program. Set VERIA_PROGRAM_ID or pass --program <pubkey>.\n',
          ),
        );
        process.exit(1);
      }
      if (!(opts.circuit in CIRCUITS)) {
        process.stderr.write(kleur.red(`Unknown circuit "${opts.circuit}"\n`));
        process.exit(1);
      }

      const proofBytes = new Uint8Array(readFileSync(opts.proofFile));
      const publicInputs = new Uint8Array(readFileSync(opts.publicFile));
      const rpc = clusterRpc(global.cluster ?? 'devnet');
      const connection = new Connection(rpc, 'confirmed');
      const verifier = new VeriaVerifier({ connection, programId: global.program });

      const recordPda = verifier.recordPda(proofBytes, publicInputs);
      process.stderr.write(
        `${kleur.gray('cluster')}      ${global.cluster ?? 'devnet'}\n` +
          `${kleur.gray('program')}      ${global.program}\n` +
          `${kleur.gray('record PDA')}   ${recordPda.toBase58()}\n` +
          `${kleur.gray('circuit')}      ${opts.circuit}\n\n`,
      );

      const spinner = ora({ text: 'Building verify_proof instruction...', stream: process.stderr }).start();
      try {
        const existing = await verifier.fetchRecord(recordPda);
        if (existing) {
          spinner.warn('ProofRecord PDA already exists — this proof was previously verified.');
          process.stdout.write(
            JSON.stringify(
              {
                alreadyVerified: true,
                recordPda: recordPda.toBase58(),
                circuit: opts.circuit,
                verifiedAt: existing.verifiedAt,
                explorerUrl: `https://explorer.solana.com/address/${recordPda.toBase58()}`,
              },
              null,
              2,
            ) + '\n',
          );
          return;
        }
        spinner.info('Instruction prepared. Sign and submit with your wallet:');
        process.stdout.write(
          JSON.stringify(
            {
              recordPda: recordPda.toBase58(),
              programId: global.program,
              circuit: opts.circuit,
              hint: 'Use @solana/web3.js sendAndConfirmTransaction with VeriaVerifier.buildVerifyTx().',
            },
            null,
            2,
          ) + '\n',
        );
      } catch (exc) {
        spinner.fail((exc as Error).message);
        process.exit(1);
      }
    });
}
