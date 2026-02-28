import { readFileSync } from 'node:fs';
import { Command } from 'commander';
import kleur from 'kleur';
import ora from 'ora';

interface DeployOpts {
  elf: string;
  name: string;
  description: string;
  maxInputSize: string;
}

export function registerDeploy(root: Command): void {
  root
    .command('deploy-circuit')
    .description('Register a new circuit ELF with the VERIA Compute API (dev preview)')
    .requiredOption('-e, --elf <path>', 'Compiled SP1 ELF file')
    .requiredOption('-n, --name <name>', 'Circuit display name')
    .requiredOption('-d, --description <text>', 'One-line description')
    .option('--max-input-size <n>', 'Maximum input length', '4096')
    .action(async (opts: DeployOpts) => {
      const elf = readFileSync(opts.elf);
      const sizeKb = (elf.byteLength / 1024).toFixed(1);
      process.stderr.write(
        `${kleur.gray('name')}        ${opts.name}\n` +
          `${kleur.gray('elf size')}    ${sizeKb} KB\n` +
          `${kleur.gray('description')} ${opts.description}\n` +
          `${kleur.gray('max input')}   ${opts.maxInputSize}\n\n`,
      );

      const spinner = ora({ text: 'Uploading circuit ELF...', stream: process.stderr }).start();
      spinner.info(
        'deploy-circuit is a dev preview. Circuit onboarding requires a signed governance proposal in v0.1.0; ELF buffered locally.',
      );
      process.stdout.write(
        JSON.stringify(
          {
            staged: true,
            name: opts.name,
            elfBytes: elf.byteLength,
            maxInputSize: Number.parseInt(opts.maxInputSize, 10),
            note: 'Submit via the governance proposal flow once registry-program is live (Phase v0.2).',
          },
          null,
          2,
        ) + '\n',
      );
    });
}
