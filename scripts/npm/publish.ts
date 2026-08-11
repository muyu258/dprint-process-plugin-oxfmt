import { assert } from "@std/assert";
import { basename, join } from "@std/path";
import {
  assertReleaseVersions,
  commandText,
  MAIN_PACKAGE_NAME,
  REPOSITORY,
  repositoryRoot,
} from "../release/common.ts";

interface ReleaseArtifact {
  package: string;
  tarball: string;
}

interface NpmRelease {
  version: string;
  platforms: ReleaseArtifact[];
  main: ReleaseArtifact;
}

interface CommandResult {
  success: boolean;
  stdout: string;
  stderr: string;
}

async function npm(args: string[]): Promise<CommandResult> {
  const output = await new Deno.Command("npm", { args, stdout: "piped", stderr: "piped" }).output();
  return {
    success: output.success,
    stdout: new TextDecoder().decode(output.stdout),
    stderr: new TextDecoder().decode(output.stderr),
  };
}

function expectedRepository(value: string): boolean {
  const normalized = value
    .trim()
    .replace(/^git\+/, "")
    .replace(/\.git$/, "");
  return new Set([
    `https://github.com/${REPOSITORY}`,
    `git@github.com:${REPOSITORY}`,
    `ssh://git@github.com/${REPOSITORY}`,
  ]).has(normalized);
}

function isNotFound(result: CommandResult): boolean {
  // npm versions differ on whether registry errors are written to stdout or stderr.
  return !result.success && /\bE404\b/.test(`${result.stdout}\n${result.stderr}`);
}

function commandFailureDetails(result: CommandResult): string {
  const output = [result.stderr.trim(), result.stdout.trim()].filter(Boolean).join("\n");
  return output.length === 0 ? "" : `\n${output}`;
}

export async function readNpmRelease(): Promise<NpmRelease> {
  const version = await assertReleaseVersions();
  const output = join(repositoryRoot, "dist/npm");
  const artifacts: ReleaseArtifact[] = [];
  const packages = new Set<string>();

  for await (const entry of Deno.readDir(output)) {
    if (!entry.isFile || !entry.name.endsWith(".tgz")) continue;
    const tarball = join(output, entry.name);
    const manifest = JSON.parse(
      await commandText("tar", ["-xOzf", tarball, "package/package.json"]),
    ) as Record<string, unknown>;
    assert(
      typeof manifest.name === "string" && manifest.version === version,
      `${entry.name} package identity/version mismatch`,
    );
    assert(!packages.has(manifest.name), `Duplicate package tarball: ${manifest.name}`);
    packages.add(manifest.name);
    artifacts.push({ package: manifest.name, tarball });
  }

  const main = artifacts.filter((artifact) => artifact.package === MAIN_PACKAGE_NAME);
  assert(main.length === 1, `Expected exactly one ${MAIN_PACKAGE_NAME} tarball`);
  const platforms = artifacts.filter((artifact) => artifact.package !== MAIN_PACKAGE_NAME);
  assert(platforms.length > 0, "No npm platform tarballs found");
  const platformPrefix = `${MAIN_PACKAGE_NAME}-`;
  assert(
    platforms.every((artifact) => artifact.package.startsWith(platformPrefix)),
    `Platform package names must start with ${platformPrefix}`,
  );
  platforms.sort((a, b) => a.package.localeCompare(b.package));
  return { version, platforms, main: main[0] };
}

export async function publishRelease(
  release: NpmRelease,
  options: { preflight?: boolean; publishArgs?: string[] } = {},
): Promise<void> {
  const artifacts: ReleaseArtifact[] = [...release.platforms, release.main];
  if (options.preflight !== false) {
    // Finish every availability check before publishing any part of the release set.
    for (const artifact of artifacts) {
      const repository = await npm(["view", artifact.package, "repository.url"]);
      assert(
        repository.success && expectedRepository(repository.stdout),
        `${artifact.package} is not bootstrapped for this repository`,
      );
      const existing = await npm(["view", `${artifact.package}@${release.version}`, "version"]);
      assert(!existing.success, `${artifact.package}@${release.version} is already published`);
      assert(
        isNotFound(existing),
        `Could not confirm version availability for ${artifact.package}@${release.version}`,
      );
    }
  }
  // Publish every platform package first so the main package is always installable when visible.
  for (const artifact of artifacts) {
    const args = [
      "publish",
      ...(options.publishArgs ?? ["--access", "public", "--provenance"]),
      artifact.tarball,
    ];
    const result = await npm(args);
    assert(
      result.success,
      `npm publish failed for ${artifact.package} (${basename(artifact.tarball)})${commandFailureDetails(result)}`,
    );
  }
}

if (import.meta.main) await publishRelease(await readNpmRelease());
