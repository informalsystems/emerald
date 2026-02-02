#!/usr/bin/env -S deno run -A
/**
 * Emerald Simplex Devnet CLI
 *
 * A CLI tool to manage a local emerald-simplex devnet with Docker.
 *
 * Usage:
 *   deno run -A devnet.ts start
 *   deno run -A devnet.ts stop
 *   deno run -A devnet.ts logs
 *   deno run -A devnet.ts monitor
 */

import { Command } from "jsr:@cliffy/command@1.0.0-rc.7";
import { colors } from "jsr:@cliffy/ansi@1.0.0-rc.7/colors";
import { ensureDir, exists } from "jsr:@std/fs@1";
import { dirname, fromFileUrl, join } from "jsr:@std/path@1";
import { delay } from "jsr:@std/async@1";
import * as yaml from "jsr:@std/yaml@1";
import * as toml from "jsr:@std/toml@1";
import { encodeHex } from "jsr:@std/encoding@1/hex";
import { consola } from "npm:consola@3";
import ip from "npm:ip@2";
import {
  type Chain,
  createPublicClient,
  createWalletClient,
  defineChain,
  formatEther,
  http,
  parseEther,
  type PublicClient,
} from "npm:viem@2";
import { HDKey, privateKeyToAccount } from "npm:viem@2/accounts";
import { mnemonicToSeedSync } from "npm:@scure/bip39@1";

// Constants
const DEFAULT_PROJECT_NAME = "simplex-devnet";
const DEFAULT_VALIDATORS = 4;
const PREFUNDED_COUNT = 10;
const ANVIL_MNEMONIC =
  "test test test test test test test test test test test junk";

type NetworkConfig = {
  subnet: string;
  networkAddress: string;
  broadcastAddress: string;
  subnetMaskLength: number;
  length: number;
};

function getProjectName(): string {
  const envName = Deno.env.get("SIMPLEX_DOCKER_PROJECT");
  if (envName && envName.trim()) {
    return envName.trim();
  }
  return DEFAULT_PROJECT_NAME;
}

// Create HD key once for efficient derivation
const masterSeed = mnemonicToSeedSync(ANVIL_MNEMONIC);
const masterHdKey = HDKey.fromMasterSeed(masterSeed);

function deriveAccount(
  index: number,
): { address: `0x${string}`; privateKey: `0x${string}` } {
  const path = `m/44'/60'/0'/0/${index}`;
  const derived = masterHdKey.derive(path);
  const privateKey = `0x${encodeHex(derived.privateKey!)}` as `0x${string}`;
  const account = privateKeyToAccount(privateKey);
  return { address: account.address, privateKey };
}

function parseCidr(cidr: string): NetworkConfig {
  const parsed = ip.cidrSubnet(cidr);
  return {
    subnet: `${parsed.networkAddress}/${parsed.subnetMaskLength}`,
    networkAddress: parsed.networkAddress,
    broadcastAddress: parsed.broadcastAddress,
    subnetMaskLength: parsed.subnetMaskLength,
    length: parsed.length,
  };
}

function cidrOverlaps(a: NetworkConfig, b: NetworkConfig): boolean {
  const aStart = ip.toLong(a.networkAddress);
  const aEnd = ip.toLong(a.broadcastAddress);
  const bStart = ip.toLong(b.networkAddress);
  const bEnd = ip.toLong(b.broadcastAddress);
  return aStart <= bEnd && bStart <= aEnd;
}

function ensureSubnetCapacity(
  cidr: NetworkConfig,
  numValidators: number,
): void {
  const maxOffset = 10 + numValidators - 1;
  if (maxOffset >= cidr.length) {
    throw new Error(
      `Subnet ${cidr.subnet} is too small for ${numValidators} validators`,
    );
  }
}

function ipAtOffset(cidr: NetworkConfig, offset: number): string {
  return ip.fromLong(ip.toLong(cidr.networkAddress) + offset);
}

// Get script directory
function getScriptDir(): string {
  const scriptPath = fromFileUrl(import.meta.url);
  return dirname(scriptPath);
}

// Generate genesis.json for the devnet
function generateGenesis(): object {
  const alloc: Record<string, { balance: string }> = {};
  for (let i = 0; i < PREFUNDED_COUNT; i++) {
    const { address } = deriveAccount(i);
    // Reth requires addresses without 0x prefix in genesis alloc
    alloc[address.toLowerCase().slice(2)] = {
      balance: "0x21e19e0c9bab2400000", // 10000 ETH
    };
  }

  return {
    config: {
      chainId: 1,
      homesteadBlock: 0,
      eip150Block: 0,
      eip155Block: 0,
      eip158Block: 0,
      byzantiumBlock: 0,
      constantinopleBlock: 0,
      petersburgBlock: 0,
      istanbulBlock: 0,
      berlinBlock: 0,
      londonBlock: 0,
      arrowGlacierBlock: 0,
      grayGlacierBlock: 0,
      mergeNetsplitBlock: 0,
      shanghaiTime: 0,
      cancunTime: 0,
      pragueTime: 0,
      terminalTotalDifficulty: 0,
      terminalTotalDifficultyPassed: true,
    },
    nonce: "0x0",
    timestamp: "0x0",
    extraData: "0x",
    gasLimit: "0x1c9c380",
    difficulty: "0x0",
    mixHash:
      "0x0000000000000000000000000000000000000000000000000000000000000000",
    coinbase: "0x0000000000000000000000000000000000000000",
    alloc,
    baseFeePerGas: "0x3b9aca00",
    blobGasUsed: "0x0",
    excessBlobGas: "0x0",
  };
}

// Generate docker-compose.yml dynamically
function generateDockerCompose(
  numValidators: number,
  network: NetworkConfig,
  uid: number,
  gid: number,
): string {
  const services: Record<string, unknown> = {};

  for (let i = 0; i < numValidators; i++) {
    const httpPort = 8545 + i * 100;
    const authPort = 8551 + i * 100;
    const p2pPort = 30303 + i * 100;
    const simplexP2pPort = 9000 + i * 2;
    const simplexMetricsPort = 9001 + i * 2;
    const otterscanPort = 5100 + i;
    const ip = ipAtOffset(network, 10 + i);

    services[`reth-${i}`] = {
      image: "ghcr.io/informalsystems/custom-reth:latest",
      container_name: `reth-${i}`,
      user: `${uid}:${gid}`,
      working_dir: "/data",
      restart: "unless-stopped",
      networks: ["simplex-net"],
      environment: [
        "HOME=/data",
        "XDG_STATE_HOME=/data",
        "XDG_DATA_HOME=/data",
      ],
      command: [
        "node",
        "--chain=/config/genesis.json",
        "--datadir=/data",
        "--http",
        "--http.addr=0.0.0.0",
        "--http.port=8545",
        "--http.api=eth,net,web3,debug,trace,txpool,ots",
        "--http.corsdomain=*",
        "--authrpc.addr=0.0.0.0",
        "--authrpc.port=8551",
        "--authrpc.jwtsecret=/config/jwt.hex",
        "--port=30303",
        "--disable-discovery",
        "--ipcdisable",
        "--metrics=0.0.0.0:9001",
      ],
      ports: [`${httpPort}:8545`, `${authPort}:8551`, `${p2pPort}:30303`],
      volumes: [
        `./data/reth-${i}:/data`,
        `./simplex-${i}/config:/config:ro`,
      ],
    };

    services[`simplex-${i}`] = {
      image: "emerald-simplex:latest",
      container_name: `simplex-${i}`,
      user: `${uid}:${gid}`,
      working_dir: "/data",
      restart: "unless-stopped",
      networks: {
        "simplex-net": { ipv4_address: ip },
      },
      command: [
        `--config=/config/validator.toml`,
        `--peers=/config/peers.yaml`,
      ],
      environment: [
        "RUST_LOG=info,emerald_simplex=debug",
        `SIMPLEX_LOG_FILE=/logs/simplex-${i}.log`,
      ],
      ports: [
        `${simplexP2pPort}:${simplexP2pPort}`,
        `${19000 + simplexMetricsPort}:${simplexMetricsPort}`,
      ],
      volumes: [
        `./data/simplex-${i}:/data`,
        `./simplex-${i}/config:/config:ro`,
        "./logs:/logs",
      ],
      depends_on: [`reth-${i}`],
    };

    services[`otterscan-${i}`] = {
      image: "otterscan/otterscan:develop",
      container_name: `otterscan-${i}`,
      restart: "unless-stopped",
      networks: ["simplex-net"],
      environment: ["DISABLE_CONFIG_OVERWRITE=1"],
      ports: [`${otterscanPort}:80`],
      volumes: [`./config/otterscan-${i}.json:/config.json:ro`],
      depends_on: [`reth-${i}`],
    };
  }

  const compose = {
    services,
    networks: {
      "simplex-net": {
        driver: "bridge",
        ipam: {
          config: [{ subnet: network.subnet }],
        },
      },
    },
  };

  return yaml.stringify(compose);
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

// Run a command and return output
async function runCommand(
  cmd: string[],
  options?: {
    cwd?: string;
    quiet?: boolean;
    inherit?: boolean;
    env?: Record<string, string>;
  },
): Promise<{ code: number; stdout: string; stderr: string }> {
  if (options?.inherit) {
    const process = new Deno.Command(cmd[0], {
      args: cmd.slice(1),
      cwd: options?.cwd,
      env: options?.env,
      stdout: "inherit",
      stderr: "inherit",
    }).spawn();
    const status = await process.status;
    return { code: status.code, stdout: "", stderr: "" };
  }

  const process = new Deno.Command(cmd[0], {
    args: cmd.slice(1),
    cwd: options?.cwd,
    env: options?.env,
    stdout: "piped",
    stderr: "piped",
  });

  const { code, stdout, stderr } = await process.output();
  const stdoutStr = new TextDecoder().decode(stdout);
  const stderrStr = new TextDecoder().decode(stderr);

  if (!options?.quiet && code !== 0) {
    consola.error(stderrStr);
  }

  return { code, stdout: stdoutStr, stderr: stderrStr };
}

async function getExistingDockerSubnets(): Promise<NetworkConfig[]> {
  const listResult = await runCommand(["docker", "network", "ls", "-q"], {
    quiet: true,
  });
  if (listResult.code !== 0) {
    return [];
  }
  const ids = listResult.stdout.trim().split(/\s+/).filter(Boolean);
  if (ids.length === 0) {
    return [];
  }
  const inspectResult = await runCommand(
    ["docker", "network", "inspect", ...ids],
    { quiet: true },
  );
  if (inspectResult.code !== 0) {
    return [];
  }
  let data: Array<{ IPAM?: { Config?: Array<{ Subnet?: string }> } }> = [];
  try {
    data = JSON.parse(inspectResult.stdout);
  } catch (error) {
    consola.warn(
      `Failed to parse Docker network inspect output: ${formatError(error)}`,
    );
    return [];
  }
  const subnets: NetworkConfig[] = [];
  for (const net of data) {
    const configs = net?.IPAM?.Config ?? [];
    for (const config of configs) {
      if (typeof config?.Subnet === "string") {
        try {
          subnets.push(parseCidr(config.Subnet));
        } catch (error) {
          consola.warn(
            `Ignoring invalid Docker network subnet "${config.Subnet}": ${
              formatError(error)
            }`,
          );
        }
      }
    }
  }
  return subnets;
}

async function getProjectNetworkSubnet(
  projectName: string,
): Promise<NetworkConfig | null> {
  const networkName = `${projectName}_simplex-net`;
  const inspectResult = await runCommand(
    ["docker", "network", "inspect", networkName],
    { quiet: true },
  );
  if (inspectResult.code !== 0) {
    return null;
  }
  let data: Array<{ IPAM?: { Config?: Array<{ Subnet?: string }> } }> = [];
  try {
    data = JSON.parse(inspectResult.stdout);
  } catch (error) {
    consola.warn(
      `Failed to parse Docker network inspect output: ${formatError(error)}`,
    );
    return null;
  }
  const config = data?.[0]?.IPAM?.Config?.[0];
  if (config?.Subnet) {
    try {
      return parseCidr(config.Subnet);
    } catch (error) {
      consola.warn(
        `Ignoring invalid Docker network subnet "${config.Subnet}": ${
          formatError(error)
        }`,
      );
      return null;
    }
  }
  return null;
}

function buildCandidateSubnets(): string[] {
  const candidates: string[] = [];
  for (let b = 31; b >= 18; b--) {
    for (let c = 0; c <= 255; c++) {
      candidates.push(`172.${b}.${c}.0/24`);
    }
  }
  return candidates;
}

async function resolveSubnet(
  numValidators: number,
  projectName: string,
  requested?: string,
): Promise<NetworkConfig> {
  const envSubnet = Deno.env.get("SIMPLEX_DOCKER_SUBNET");
  const desired = requested ?? envSubnet;
  if (desired) {
    const cidr = parseCidr(desired);
    ensureSubnetCapacity(cidr, numValidators);
    return cidr;
  }

  const existingProject = await getProjectNetworkSubnet(projectName);
  if (existingProject) {
    ensureSubnetCapacity(existingProject, numValidators);
    return existingProject;
  }

  const existing = await getExistingDockerSubnets();
  const defaultSubnet = parseCidr("172.28.0.0/16");
  ensureSubnetCapacity(defaultSubnet, numValidators);
  if (!existing.some((used) => cidrOverlaps(defaultSubnet, used))) {
    return defaultSubnet;
  }

  const candidates = buildCandidateSubnets();
  for (const candidate of candidates) {
    let cidr: NetworkConfig;
    try {
      cidr = parseCidr(candidate);
    } catch (error) {
      consola.debug(
        `Skipping invalid subnet candidate ${candidate}: ${formatError(error)}`,
      );
      continue;
    }
    ensureSubnetCapacity(cidr, numValidators);
    if (!existing.some((used) => cidrOverlaps(cidr, used))) {
      return cidr;
    }
  }

  throw new Error(
    "Unable to find a free Docker subnet. Set SIMPLEX_DOCKER_SUBNET to override.",
  );
}

// Run docker compose command
async function dockerCompose(
  args: string[],
  runDir: string,
  options?: { projectName?: string | null },
): Promise<{ code: number; stdout: string; stderr: string }> {
  const composeFile = join(runDir, "docker-compose.yml");
  const cmd = [
    "docker",
    "compose",
    "-f",
    composeFile,
    "--project-directory",
    runDir,
  ];
  if (options?.projectName !== null) {
    const projectName = options?.projectName ?? getProjectName();
    cmd.push("--project-name", projectName);
  }
  cmd.push(...args);
  return await runCommand(cmd, { cwd: runDir, inherit: true });
}

// Define devnet chain for viem clients
const devnetChain: Chain = defineChain({
  id: 1,
  name: "Emerald Simplex Devnet",
  nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
  rpcUrls: {
    default: { http: ["http://localhost:8545"] },
  },
});

// Create a public client for a given RPC URL
function getPublicClient(rpcUrl: string): PublicClient {
  return createPublicClient({
    chain: devnetChain,
    transport: http(rpcUrl),
  });
}

// Wait for RPC endpoint to be ready
async function waitForRpc(url: string, maxRetries = 120): Promise<boolean> {
  const client = getPublicClient(url);
  for (let i = 0; i < maxRetries; i++) {
    try {
      await client.getBlockNumber();
      return true;
    } catch (error) {
      consola.debug(`RPC not ready at ${url}: ${formatError(error)}`);
    }
    await Deno.stdout.write(new TextEncoder().encode("."));
    await delay(1000);
  }
  return false;
}

// Get genesis hash from Reth
async function getGenesisHash(rpcUrl: string): Promise<string> {
  const client = getPublicClient(rpcUrl);
  const block = await client.getBlock({ blockNumber: 0n });
  return block.hash;
}

// Get current block number
async function getBlockNumber(rpcUrl: string): Promise<bigint | null> {
  try {
    const client = getPublicClient(rpcUrl);
    return await client.getBlockNumber();
  } catch (error) {
    consola.debug(
      `Failed to get block number from ${rpcUrl}: ${formatError(error)}`,
    );
    return null;
  }
}

// Create run directory with timestamp
async function createRunDir(scriptDir: string): Promise<string> {
  const runsDir = join(scriptDir, "runs");
  await ensureDir(runsDir);

  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  const runDir = join(runsDir, timestamp);
  await ensureDir(runDir);
  await ensureDir(join(runDir, "config"));
  await ensureDir(join(runDir, "logs"));

  // Create symlink to latest run
  const latestLink = join(runsDir, "latest");
  try {
    await Deno.remove(latestLink);
  } catch (error) {
    if (!(error instanceof Deno.errors.NotFound)) {
      consola.warn(`Failed to remove latest symlink: ${formatError(error)}`);
    }
  }
  await Deno.symlink(runDir, latestLink);

  return runDir;
}

// Get latest run directory
async function getLatestRunDir(scriptDir: string): Promise<string | null> {
  const latestLink = join(scriptDir, "runs", "latest");
  try {
    const target = await Deno.readLink(latestLink);
    if (await exists(target)) {
      return target;
    }
    // If symlink target doesn't exist, try to resolve relative path
    const resolved = join(dirname(latestLink), target);
    if (await exists(resolved)) {
      return resolved;
    }
  } catch (error) {
    if (!(error instanceof Deno.errors.NotFound)) {
      consola.warn(`Failed to read latest symlink: ${formatError(error)}`);
    }
  }
  return null;
}

async function getGenesisAlloc(): Promise<
  Record<string, { balance: string }> | null
> {
  const scriptDir = getScriptDir();
  const runDir = await getLatestRunDir(scriptDir);
  if (!runDir) {
    return null;
  }
  const genesisPath = join(runDir, "config", "genesis.json");
  if (!(await exists(genesisPath))) {
    return null;
  }
  try {
    const content = await Deno.readTextFile(genesisPath);
    const genesis = JSON.parse(content) as {
      alloc?: Record<string, { balance: string }>;
    };
    return genesis.alloc ?? null;
  } catch (error) {
    consola.warn(`Failed to read genesis file: ${formatError(error)}`);
    return null;
  }
}

// Build Docker image
async function buildImage(scriptDir: string): Promise<void> {
  consola.start("Building emerald-simplex Docker image...");

  const emeraldRoot = join(scriptDir, "..", "..");
  const dockerfile = join(scriptDir, "Dockerfile");

  const result = await runCommand(
    [
      "docker",
      "build",
      "-t",
      "emerald-simplex:latest",
      "-f",
      dockerfile,
      emeraldRoot,
    ],
    {
      inherit: true,
    },
  );

  if (result.code !== 0) {
    throw new Error("Failed to build Docker image");
  }

  consola.info("Image built successfully");
}

async function pullImage(image: string): Promise<void> {
  consola.start(`Pulling ${image}...`);
  const result = await runCommand(["docker", "pull", image], { inherit: true });
  if (result.code !== 0) {
    throw new Error(`Failed to pull ${image}`);
  }
  consola.info(`Pulled ${image}`);
}

async function buildDevnet(): Promise<void> {
  const scriptDir = getScriptDir();
  await buildImage(scriptDir);
  await pullImage("ghcr.io/informalsystems/custom-reth:latest");
  await pullImage("otterscan/otterscan:v2.6.1");
}

// Generate validator configs
async function generateConfigs(
  runDir: string,
  numValidators: number,
  network: NetworkConfig,
): Promise<void> {
  consola.start("Generating simplex validator configs...");

  const configDir = join(runDir, "config");

  // Use a temp directory name that will be created by the tool in Docker
  const tempDirName = "config-temp";
  const tempDir = join(runDir, tempDirName);

  // Clean up temp directory if it exists from a previous run
  try {
    await Deno.remove(tempDir, { recursive: true });
  } catch (error) {
    if (!(error instanceof Deno.errors.NotFound)) {
      consola.warn(
        `Failed to remove temp directory ${tempDir}: ${formatError(error)}`,
      );
    }
  }

  // Run setup tool inside Docker container - mount the runDir as /output parent
  // and let the tool create the subdirectory
  const result = await runCommand([
    "docker",
    "run",
    "--rm",
    "-v",
    `${runDir}:/output`,
    "-u",
    `${Deno.uid()}:${Deno.gid()}`,
    "--entrypoint",
    "/usr/local/bin/emerald-simplex-setup",
    "emerald-simplex:latest",
    "generate",
    "--validators",
    numValidators.toString(),
    "--output",
    `/output/${tempDirName}`,
    "--base-port",
    "9000",
    "--base-engine-port",
    "8551",
    "--fee-recipient",
    deriveAccount(0).address,
    "--eth-genesis-path",
    "/output/config/genesis.json",
  ], { inherit: true });

  if (result.code !== 0) {
    throw new Error("Failed to generate configs");
  }

  // Copy peers.yaml from temp dir to config dir (setup.rs generates it at the output root)
  const srcPeers = join(tempDir, "peers.yaml");
  const destPeers = join(configDir, "peers.yaml");
  if (await exists(srcPeers)) {
    await Deno.copyFile(srcPeers, destPeers);
    consola.debug("Copied peers.yaml to config directory");
  }

  // Copy JWT files and validator configs from each validator's directory
  consola.info("Copying JWT files and configs to validator directories...");
  for (let i = 0; i < numValidators; i++) {
    const validatorConfigDir = join(runDir, `simplex-${i}`, "config");
    await ensureDir(validatorConfigDir);

    // Copy JWT file from validator-{i}/config/jwt.hex (setup.rs structure)
    const srcJwt = join(tempDir, `validator-${i}`, "config", "jwt.hex");
    const destJwt = join(validatorConfigDir, "jwt.hex");
    if (await exists(srcJwt)) {
      await Deno.copyFile(srcJwt, destJwt);
      consola.debug(`Copied jwt.hex to simplex-${i}/config/`);
    } else {
      consola.warn(`JWT file not found for validator-${i} at ${srcJwt}`);
    }

    // Copy validator toml config from validator-{i}/config/validator.toml
    const srcToml = join(tempDir, `validator-${i}`, "config", "validator.toml");
    const destToml = join(validatorConfigDir, "validator.toml");
    if (await exists(srcToml)) {
      await Deno.copyFile(srcToml, destToml);
      consola.debug(`Copied validator.toml to simplex-${i}/config/`);
    } else {
      consola.warn(`validator.toml not found for validator-${i} at ${srcToml}`);
    }
  }

  // Clean up temp directory
  try {
    await Deno.remove(tempDir, { recursive: true });
  } catch (error) {
    consola.warn(
      `Failed to remove temp directory ${tempDir}: ${formatError(error)}`,
    );
  }

  // Update each validator toml with correct paths
  consola.info("Updating validator TOML configs...");

  for (let i = 0; i < numValidators; i++) {
    const validatorConfigDir = join(runDir, `simplex-${i}`, "config");
    const tomlPath = join(validatorConfigDir, `validator.toml`);

    // Read and modify the toml config
    const content = await Deno.readTextFile(tomlPath);
    const config = toml.parse(content) as Record<string, unknown>;

    const simplex = (config.simplex ?? {}) as Record<string, unknown>;
    simplex.directory = "/data";
    config.simplex = simplex;

    config.execution_authrpc_address = `http://reth-${i}:8545`;
    config.engine_authrpc_address = `http://reth-${i}:8551`;
    config.jwt_token_path = "/config/jwt.hex";

    await Deno.writeTextFile(tomlPath, toml.stringify(config));
    consola.debug(`Updated validator.toml`);
  }

  // Update peers file to use Docker static IPs and copy to each validator
  const peersFile = join(configDir, "peers.yaml");
  const peersContent = await Deno.readTextFile(peersFile);
  const peers = yaml.parse(peersContent) as {
    addresses: Record<string, string>;
  };

  const keys = Object.keys(peers.addresses).sort();
  const newAddresses: Record<string, string> = {};

  for (let i = 0; i < keys.length; i++) {
    const port = 9000 + i * 2;
    const ip = ipAtOffset(network, 10 + i);
    newAddresses[keys[i]] = `${ip}:${port}`;
  }

  peers.addresses = newAddresses;

  // Write updated peers.yaml to each validator's config directory
  for (let i = 0; i < numValidators; i++) {
    const validatorConfigDir = join(runDir, `simplex-${i}`, "config");
    const validatorPeersFile = join(validatorConfigDir, `peers.yaml`);
    await Deno.writeTextFile(validatorPeersFile, yaml.stringify(peers));
    consola.debug(`Created peers.yaml for simplex-${i}`);
  }

  consola.info(`Updated peers.yaml with Docker static IPs (${network.subnet})`);

  // Copy genesis.json to each validator's config directory for reth
  const genesisPath = join(configDir, "genesis.json");
  if (await exists(genesisPath)) {
    consola.info("Copying genesis.json to validator config directories...");
    for (let i = 0; i < numValidators; i++) {
      const validatorConfigDir = join(runDir, `simplex-${i}`, "config");
      await ensureDir(validatorConfigDir);
      const destGenesis = join(validatorConfigDir, "genesis.json");
      await Deno.copyFile(genesisPath, destGenesis);
      consola.debug(`Copied genesis.json to simplex-${i}/config/`);
    }
  }

  consola.info(`Configs generated in ${configDir}`);
}

// Start command
async function startDevnet(options: {
  clean?: boolean;
  cleanAll?: boolean;
  subnet?: string;
  validators?: number;
}): Promise<void> {
  const scriptDir = getScriptDir();
  const numValidators = options.validators ?? DEFAULT_VALIDATORS;
  const projectName = getProjectName();

  await stopDevnetIfRunning();

  // Clean up if requested
  if (options.clean) {
    consola.start("Cleaning up previous run...");
    const latestRunDir = await getLatestRunDir(scriptDir);
    if (latestRunDir) {
      await dockerCompose(
        ["down", "-v", "--remove-orphans"],
        latestRunDir,
        { projectName },
      );
    }
    consola.info("Cleanup complete");
  }

  if (options.cleanAll) {
    await cleanAllRuns();
  }

  const network = await resolveSubnet(
    numValidators,
    projectName,
    options.subnet,
  );

  // Create new run directory
  const runDir = await createRunDir(scriptDir);
  consola.info(`Run directory: ${runDir}`);
  consola.info(`Number of validators: ${numValidators}`);
  consola.info(`Docker project: ${projectName}`);
  consola.info(`Docker subnet: ${network.subnet}`);

  const uid = Deno.uid();
  const gid = Deno.gid();
  if (uid === null || gid === null) {
    throw new Error("Unable to determine user/group IDs for Docker containers");
  }

  // Ensure data/config directories exist for bind mounts
  await ensureDir(join(runDir, "data"));
  for (let i = 0; i < numValidators; i++) {
    await ensureDir(join(runDir, "data", `reth-${i}`));
    await ensureDir(join(runDir, "data", `simplex-${i}`));
    await ensureDir(join(runDir, `simplex-${i}`, "config"));
  }

  // Generate docker-compose.yml
  consola.start("Generating docker-compose.yml...");
  const composeContent = generateDockerCompose(
    numValidators,
    network,
    uid,
    gid,
  );
  await Deno.writeTextFile(join(runDir, "docker-compose.yml"), composeContent);

  // Generate genesis.json
  consola.start("Generating genesis.json...");
  const genesis = generateGenesis();
  await Deno.writeTextFile(
    join(runDir, "config", "genesis.json"),
    JSON.stringify(genesis, null, 2),
  );

  // Copy genesis.json to each validator config directory before starting reth
  const genesisPath = join(runDir, "config", "genesis.json");
  for (let i = 0; i < numValidators; i++) {
    const validatorConfigDir = join(runDir, `simplex-${i}`, "config");
    const destGenesis = join(validatorConfigDir, "genesis.json");
    await Deno.copyFile(genesisPath, destGenesis);
  }

  // Generate Otterscan configs
  for (let i = 0; i < numValidators; i++) {
    const httpPort = 8545 + i * 100;
    const otterscanConfig = {
      erigonURL: `http://localhost:${httpPort}`,
      chainId: 1,
    };
    await Deno.writeTextFile(
      join(runDir, "config", `otterscan-${i}.json`),
      JSON.stringify(otterscanConfig, null, 2),
    );
  }

  // Build image
  await buildImage(scriptDir);

  // Generate configs before starting reth (includes jwt + genesis hash)
  consola.start("Generating validator configs...");
  await generateConfigs(runDir, numValidators, network);

  // Start Reth nodes
  consola.start("Starting Reth nodes...");
  const rethServices = Array.from(
    { length: numValidators },
    (_, i) => `reth-${i}`,
  );
  await dockerCompose(
    ["up", "-d", "--force-recreate", ...rethServices],
    runDir,
    { projectName },
  );

  // Wait for Reth nodes
  consola.info("Waiting for Reth nodes to be ready...");
  const ready = await waitForRpc("http://localhost:8545");

  if (!ready) {
    consola.error("Reth nodes failed to start");
    await dockerCompose(["logs", "reth-0"], runDir, { projectName });
    throw new Error("Reth nodes failed to start");
  }

  consola.info("Reth node 0 is ready");

  // Wait for all other nodes
  for (let i = 1; i < numValidators; i++) {
    const port = 8545 + i * 100;
    await waitForRpc(`http://localhost:${port}`, 60);
  }
  consola.info("All Reth nodes are ready");

  // Start simplex validators
  consola.start("Starting Simplex validators...");
  const simplexServices = Array.from(
    { length: numValidators },
    (_, i) => `simplex-${i}`,
  );
  await dockerCompose(
    ["up", "-d", "--force-recreate", ...simplexServices],
    runDir,
    { projectName },
  );
  consola.info("Simplex validators started");

  // Start Otterscan explorers
  consola.start("Starting Otterscan explorers...");
  const otterscanServices = Array.from(
    { length: numValidators },
    (_, i) => `otterscan-${i}`,
  );
  await dockerCompose(
    ["up", "-d", "--force-recreate", ...otterscanServices],
    runDir,
    { projectName },
  );
  consola.info("Otterscan explorers started");

  // Wait a bit and show status
  await delay(5000);

  // Show container status
  await dockerCompose(["ps"], runDir, { projectName });

  // Show status
  showStatus(runDir);

  consola.info(
    "Devnet is running! Use 'deno run -A devnet.ts monitor' to watch block production.",
  );
}

// Stop command
async function stopDevnet(): Promise<void> {
  const scriptDir = getScriptDir();
  const runDir = await getLatestRunDir(scriptDir);
  const projectName = getProjectName();

  if (!runDir) {
    consola.error("No active devnet found");
    return;
  }

  consola.start("Stopping devnet...");
  await dockerCompose(["down"], runDir, { projectName });
  consola.info("Devnet stopped");
}

async function stopDevnetIfRunning(): Promise<void> {
  const scriptDir = getScriptDir();
  const runDir = await getLatestRunDir(scriptDir);
  const projectName = getProjectName();

  if (!runDir) {
    return;
  }

  await dockerCompose(["down"], runDir, { projectName });
}

// Clean command
async function cleanDevnet(): Promise<void> {
  await cleanAllRuns();
}

async function cleanAllRuns(): Promise<void> {
  const scriptDir = getScriptDir();
  const runsDir = join(scriptDir, "runs");
  const projectName = getProjectName();

  if (!(await exists(runsDir))) {
    consola.error("No runs directory found");
    return;
  }

  consola.start("Cleaning up all devnet runs...");

  const latestRunDir = await getLatestRunDir(scriptDir);
  if (latestRunDir) {
    await dockerCompose(["down", "-v", "--remove-orphans"], latestRunDir, {
      projectName,
    });
  }

  for await (const entry of Deno.readDir(runsDir)) {
    if (entry.name === "latest" || entry.isSymlink || !entry.isDirectory) {
      continue;
    }
    const runDir = join(runsDir, entry.name);
    if (!(await exists(join(runDir, "docker-compose.yml")))) {
      continue;
    }
    await dockerCompose(["down", "-v", "--remove-orphans"], runDir, {
      projectName: null,
    });
  }

  try {
    await Deno.remove(runsDir, { recursive: true });
  } catch (error) {
    consola.warn(
      `Failed to remove runs directory ${runsDir}: ${formatError(error)}`,
    );
  }

  consola.info("Cleanup complete");
}

// Logs command
async function showLogs(options: {
  service?: string;
  follow?: boolean;
}): Promise<void> {
  const scriptDir = getScriptDir();
  const runDir = await getLatestRunDir(scriptDir);
  const projectName = getProjectName();

  if (!runDir) {
    consola.error("No active devnet found");
    return;
  }

  const args = ["logs"];

  if (options.follow) {
    args.push("-f");
  }

  if (options.service) {
    args.push(options.service);
  }

  const composeFile = join(runDir, "docker-compose.yml");
  const process = new Deno.Command("docker", {
    args: [
      "compose",
      "-f",
      composeFile,
      "--project-directory",
      runDir,
      "--project-name",
      projectName,
      ...args,
    ],
    cwd: runDir,
    stdout: "inherit",
    stderr: "inherit",
  });

  const child = process.spawn();
  await child.status;
}

// Crash validator - stop simplex and reth containers for a specific validator
async function crashValidator(validatorIndex: number): Promise<void> {
  const scriptDir = getScriptDir();
  const runDir = await getLatestRunDir(scriptDir);
  const projectName = getProjectName();

  if (!runDir) {
    consola.error("No active devnet found");
    return;
  }

  const rethService = `reth-${validatorIndex}`;
  const simplexService = `simplex-${validatorIndex}`;

  consola.start(`Crashing validator ${validatorIndex}...`);

  // Stop both services
  consola.info(`Stopping ${rethService}...`);
  await dockerCompose(["stop", rethService], runDir, { projectName });

  consola.info(`Stopping ${simplexService}...`);
  await dockerCompose(["stop", simplexService], runDir, { projectName });

  consola.success(
    `Validator ${validatorIndex} crashed (both ${rethService} and ${simplexService} stopped)`,
  );
}

// Restart validator - start simplex and reth containers for a specific validator
async function restartValidator(validatorIndex: number): Promise<void> {
  const scriptDir = getScriptDir();
  const runDir = await getLatestRunDir(scriptDir);
  const projectName = getProjectName();

  if (!runDir) {
    consola.error("No active devnet found");
    return;
  }

  const rethService = `reth-${validatorIndex}`;
  const simplexService = `simplex-${validatorIndex}`;
  const httpPort = 8545 + validatorIndex * 100;

  consola.start(`Restarting validator ${validatorIndex}...`);

  // Start reth first (simplex depends on it)
  consola.info(`Starting ${rethService}...`);
  await dockerCompose(["start", rethService], runDir, { projectName });

  // Wait for reth to be fully ready before starting simplex
  consola.info(`Waiting for ${rethService} to be ready...`);
  const rethReady = await waitForRpc(`http://localhost:${httpPort}`, 60);

  if (!rethReady) {
    consola.error(`${rethService} failed to become ready`);
    consola.info(`View logs with: deno run -A devnet.ts logs ${rethService}`);
    return;
  }

  consola.info(`${rethService} is ready, starting ${simplexService}...`);
  await dockerCompose(["start", simplexService], runDir, { projectName });

  consola.success(
    `Validator ${validatorIndex} restarted (both ${rethService} and ${simplexService} started)`,
  );
}

// Monitor command
async function monitorBlocks(limit = 10): Promise<void> {
  consola.info(`Monitoring block production (up to ${limit} new blocks)...`);

  let lastBlock: bigint | null = null;
  let seen = 0n;

  while (true) {
    const blockNumber = await getBlockNumber("http://localhost:8545");

    if (blockNumber !== null) {
      if (lastBlock === null) {
        lastBlock = blockNumber;
      } else if (blockNumber > lastBlock) {
        const diff = blockNumber - lastBlock;
        consola.log(`Block: ${blockNumber} (+${diff})`);
        lastBlock = blockNumber;
        seen += diff;
        if (seen >= BigInt(limit)) {
          consola.info(`Observed ${seen} new blocks. Exiting.`);
          return;
        }
      }
    }

    await delay(2000);
  }
}

// Status command
function showStatus(runDir?: string): void {
  consola.log(
    colors.blue("════════════════════════════════════════════════════════════"),
  );
  consola.log(
    colors.blue("                   DEVNET STATUS                            "),
  );
  consola.log(
    colors.blue("════════════════════════════════════════════════════════════"),
  );

  if (runDir) {
    consola.log(`Run Directory: ${runDir}`);
  }

  consola.log(colors.green("Reth RPC Endpoints:"));
  consola.log("  Node 0: http://localhost:8545");
  consola.log("  Node 1: http://localhost:8645");
  consola.log("  Node 2: http://localhost:8745");
  consola.log("  Node 3: http://localhost:8845");

  consola.log(colors.green("Otterscan Block Explorers:"));
  consola.log("  Explorer 0: http://localhost:5100");
  consola.log("  Explorer 1: http://localhost:5101");
  consola.log("  Explorer 2: http://localhost:5102");
  consola.log("  Explorer 3: http://localhost:5103");

  consola.log(colors.green("Simplex Validator Metrics:"));
  consola.log("  Validator 0: http://localhost:19001/metrics");
  consola.log("  Validator 1: http://localhost:19003/metrics");
  consola.log("  Validator 2: http://localhost:19005/metrics");
  consola.log("  Validator 3: http://localhost:19007/metrics");

  consola.log(colors.green("Prefunded Account (Anvil default):"));
  const prefunded = deriveAccount(0);
  consola.log(`  Address:     ${prefunded.address}`);
  consola.log(`  Private Key: ${prefunded.privateKey}`);
  consola.log("  Balance:     10000 ETH");

  consola.log(colors.yellow("Commands:"));
  consola.log("  Check block number:");
  consola.log("    cast block-number --rpc-url http://localhost:8545");
  consola.log("  View logs:");
  consola.log("    deno run -A devnet.ts logs -f simplex-0");
  consola.log("    deno run -A devnet.ts logs -f reth-0");
  consola.log("  Stop devnet:");
  consola.log("    deno run -A devnet.ts stop");
  consola.log("  Clean and remove volumes:");
  consola.log("    deno run -A devnet.ts clean");
  consola.log(
    colors.blue("════════════════════════════════════════════════════════════"),
  );
}

// Status command handler
async function statusCommand(): Promise<void> {
  const scriptDir = getScriptDir();
  const runDir = await getLatestRunDir(scriptDir);

  showStatus(runDir ?? undefined);
}

async function showAccounts(count = PREFUNDED_COUNT): Promise<void> {
  const alloc = await getGenesisAlloc();
  if (alloc) {
    const scriptDir = getScriptDir();
    const runDir = await getLatestRunDir(scriptDir);
    if (runDir) {
      consola.info(`Genesis: ${join(runDir, "config", "genesis.json")}`);
    }
  } else {
    consola.warn("Genesis file not found; balances will be shown as N/A");
  }

  for (let i = 0; i < count; i++) {
    const { address, privateKey } = deriveAccount(i);
    const allocKey = address.toLowerCase().replace(/^0x/, "");
    const balance = alloc?.[allocKey]?.balance ?? "N/A";
    consola.info(
      `Account ${i}: ${address} | ${privateKey} | balance: ${balance}`,
    );
  }
}

// Main CLI
const cli = new Command()
  .name("devnet")
  .version("1.0.0")
  .description("Emerald Simplex Devnet CLI - Manage a local devnet with Docker")
  .action(() => {
    cli.showHelp();
  });

cli
  .command("start", "Start the devnet")
  .option("-c, --clean", "Clean up previous run before starting")
  .option("-C, --clean-all", "Clean up all devnet runs before starting")
  .option(
    "-n, --validators <validators:number>",
    "Number of validators to run",
    {
      default: DEFAULT_VALIDATORS,
    },
  )
  .option("-s, --subnet <subnet:string>", "Docker subnet CIDR (overrides auto)")
  .action(async (options) => {
    await startDevnet(options);
  });

cli
  .command("stop", "Stop the devnet")
  .action(async () => {
    await stopDevnet();
  });

cli
  .command("clean", "Stop and remove all containers and volumes")
  .action(async () => {
    await cleanDevnet();
  });

cli
  .command("clean-all", "Stop and remove all containers, volumes, and networks")
  .action(async () => {
    await cleanAllRuns();
  });

cli
  .command("build", "Build devnet images without starting containers")
  .action(async () => {
    await buildDevnet();
  });

cli
  .command("logs", "View container logs")
  .option("-f, --follow", "Follow log output")
  .arguments("[service:string]")
  .action(async (options, service) => {
    await showLogs({ service, follow: options.follow });
  });

cli
  .command("monitor", "Monitor block production")
  .action(async () => {
    await monitorBlocks();
  });

cli
  .command("status", "Show devnet status and endpoints")
  .action(async () => {
    await statusCommand();
  });

cli
  .command("accounts", "List prefunded accounts and private keys")
  .option("-n, --count <count:number>", "Number of accounts to show", {
    default: PREFUNDED_COUNT,
  })
  .action(async (options) => {
    await showAccounts(options.count);
  });

cli
  .command("tx", "Send a test transaction between prefunded accounts")
  .option("-f, --from <index:number>", "Sender account index (0-9)", {
    default: 0,
  })
  .option("-t, --to <index:number>", "Receiver account index (0-9)", {
    default: 1,
  })
  .option("-a, --amount <amount:string>", "Amount of ETH to send", {
    default: "0.1",
  })
  .option("-n, --count <count:number>", "Number of transactions to send", {
    default: 1,
  })
  .action(async (options) => {
    await sendTestTransaction(options);
  });

cli
  .command(
    "crash-validator",
    "Crash a validator by stopping its reth and simplex containers",
  )
  .arguments("<validator:number>")
  .action(async (_, validator) => {
    if (typeof validator !== "number" || validator < 0) {
      consola.error("Validator index must be a non-negative number");
      Deno.exit(1);
    }
    await crashValidator(validator);
  });

cli
  .command(
    "restart-validator",
    "Restart a validator by starting its reth and simplex containers",
  )
  .arguments("<validator:number>")
  .action(async (_, validator) => {
    if (typeof validator !== "number" || validator < 0) {
      consola.error("Validator index must be a non-negative number");
      Deno.exit(1);
    }
    await restartValidator(validator);
  });

// Send test transaction
async function sendTestTransaction(options: {
  from: number;
  to: number;
  amount: string;
  count: number;
}): Promise<void> {
  const { from, to, amount, count } = options;
  const rpcUrl = "http://localhost:8545";

  if (
    from < 0 ||
    from >= PREFUNDED_COUNT ||
    to < 0 ||
    to >= PREFUNDED_COUNT
  ) {
    consola.error(`Account index must be between 0 and ${PREFUNDED_COUNT - 1}`);
    Deno.exit(1);
  }

  if (from === to) {
    consola.error("Sender and receiver must be different accounts");
    Deno.exit(1);
  }

  const fromAccount = deriveAccount(from);
  const toAccount = deriveAccount(to);
  const { address: toAddress } = toAccount;

  const account = privateKeyToAccount(fromAccount.privateKey);

  const walletClient = createWalletClient({
    account,
    chain: devnetChain,
    transport: http(rpcUrl),
  });

  const publicClient = getPublicClient(rpcUrl);

  consola.info(`Sending ${count} transaction(s) of ${amount} ETH`);
  consola.info(`From: ${fromAccount.address} (account ${from})`);
  consola.info(`To:   ${toAddress} (account ${to})`);

  const value = parseEther(amount);

  for (let i = 0; i < count; i++) {
    try {
      const hash = await walletClient.sendTransaction({
        to: toAddress,
        value,
      });

      consola.info(`Transaction ${i + 1}/${count} sent: ${hash}`);

      // Wait for confirmation
      const receipt = await publicClient.waitForTransactionReceipt({ hash });
      consola.info(
        `  Confirmed in block ${receipt.blockNumber}, status: ${receipt.status}`,
      );
    } catch (error) {
      consola.error(`Transaction ${i + 1}/${count} failed: ${error}`);
    }

    // Small delay between transactions
    if (i < count - 1) {
      await delay(100);
    }
  }

  // Show balances after transactions
  consola.info("Account balances after transactions:");
  for (let i = 0; i < PREFUNDED_COUNT; i++) {
    const { address } = deriveAccount(i);
    const balance = await publicClient.getBalance({ address });
    consola.log(`  Account ${i}: ${formatEther(balance)} ETH`);
  }
}

// Parse and run
await cli.parse(Deno.args);
