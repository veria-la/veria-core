import { Command } from 'commander';

export interface GlobalOpts {
  apiUrl?: string;
  program?: string;
  cluster?: string;
}

export function getGlobalOpts(cmd: Command): GlobalOpts {
  let root: Command = cmd;
  while (root.parent) root = root.parent;
  return root.opts() as GlobalOpts;
}

export function clusterRpc(cluster: string): string {
  switch (cluster) {
    case 'mainnet':
    case 'mainnet-beta':
      return 'https://api.mainnet-beta.solana.com';
    case 'devnet':
      return 'https://api.devnet.solana.com';
    case 'testnet':
      return 'https://api.testnet.solana.com';
    default:
      return cluster;
  }
}
