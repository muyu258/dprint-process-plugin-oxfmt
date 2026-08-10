import { $, CargoToml } from "@dprint/automation";
import { assert } from "@std/assert";
import { move } from "@std/fs/move";
import { fromFileUrl, join } from "@std/path";
import { z } from "zod";

export const PLUGIN_NAME = "dprint-process-plugin-oxfmt";
export const MAIN_PACKAGE_NAME = PLUGIN_NAME;
export const REPOSITORY = "muyu258/dprint-process-plugin-oxfmt";
export const repositoryRoot = fromFileUrl(new URL("../../", import.meta.url));

const runtimePackageSchema = z.object({
  version: z.string(),
  dependencies: z.object({ oxfmt: z.string() }),
});

/** Ensures all release versions and the GitHub tag, when applicable, match. */
export async function assertReleaseVersions(): Promise<string> {
  const cargoVersion = new CargoToml($.path(join(repositoryRoot, "Cargo.toml"))).version();
  const runtime = runtimePackageSchema.parse(
    JSON.parse(await Deno.readTextFile(join(repositoryRoot, "runtime/package.json"))),
  );
  const runtimeVersion = runtime.version;
  const oxfmtVersion = runtime.dependencies.oxfmt;
  assert(
    oxfmtVersion === runtimeVersion,
    `Runtime dependencies.oxfmt must exactly match Runtime version ${runtimeVersion}`,
  );
  assert(
    cargoVersion === runtimeVersion,
    `Version mismatch: Cargo=${cargoVersion}, Runtime=${runtimeVersion}, Oxfmt=${oxfmtVersion}`,
  );
  if (Deno.env.get("GITHUB_REF_TYPE") === "tag") {
    const releaseTag = Deno.env.get("GITHUB_REF_NAME") ?? "<missing>";
    assert(
      releaseTag === `v${cargoVersion}`,
      `Release tag mismatch: expected v${cargoVersion}, found ${releaseTag}`,
    );
  }
  return cargoVersion;
}

interface CommandOptions {
  cwd?: string;
  env?: Record<string, string>;
  stdout?: "inherit" | "piped" | "null";
  stderr?: "inherit" | "piped" | "null";
}

/** Runs a command and throws when it fails. */
export async function run(
  command: string,
  args: string[],
  options: CommandOptions = {},
): Promise<Deno.CommandOutput> {
  const output = await new Deno.Command(command, {
    args,
    cwd: options.cwd,
    env: options.env,
    stdout: options.stdout ?? "inherit",
    stderr: options.stderr ?? "inherit",
  }).output();
  if (!output.success) {
    const detail = output.stderr.length === 0 ? "" : `\n${new TextDecoder().decode(output.stderr)}`;
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${output.code}${detail}`);
  }
  return output;
}

/** Runs a command and returns its trimmed stdout. */
export async function commandText(command: string, args: string[], cwd?: string): Promise<string> {
  const output = await run(command, args, { cwd, stdout: "piped", stderr: "piped" });
  return new TextDecoder().decode(output.stdout).trim();
}

/** Publishes complete staged output without leaving stale files from an earlier build. */
export async function replaceDirectory(staged: string, destination: string): Promise<void> {
  await move(staged, destination, { overwrite: true });
}
