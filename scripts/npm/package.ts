import { processPlugin } from "@dprint/automation";
import { assert } from "@std/assert";
import { join } from "@std/path";
import {
  assertReleaseVersions,
  commandText,
  MAIN_PACKAGE_NAME,
  PLUGIN_NAME,
  replaceDirectory,
  repositoryRoot,
  run,
} from "../release/common.ts";

interface PlatformPayload {
  platform: processPlugin.Platform;
  directory: string;
  executable: string;
}

async function discoverPlatformInputs(
  inputDir: string,
  extractRoot: string,
): Promise<PlatformPayload[]> {
  const entries = [];
  for await (const entry of Deno.readDir(inputDir)) {
    if (entry.isDirectory) entries.push(entry);
  }
  assert(entries.length > 0, "No npm platform inputs found");

  const results: PlatformPayload[] = [];
  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
    const platform = entry.name as processPlugin.Platform;
    const expectedZip = processPlugin.getStandardZipFileName(PLUGIN_NAME, platform);
    const platformDir = join(inputDir, entry.name);
    const zipPath = join(platformDir, expectedZip);
    const destination = join(extractRoot, platform);
    await Deno.mkdir(destination, { recursive: true });
    await run("unzip", ["-q", zipPath, "-d", destination]);
    const executable = `${PLUGIN_NAME}${platform.startsWith("windows-") ? ".exe" : ""}`;
    results.push({ platform, directory: destination, executable });
  }
  return results;
}

async function createNpmPackages(): Promise<void> {
  const version = await assertReleaseVersions();
  const dist = join(repositoryRoot, "dist");
  const stage = await Deno.makeTempDir({ dir: dist, prefix: ".npm-stage-" });
  const extraction = join(stage, "payloads");
  const output = join(stage, "npm");
  await Deno.mkdir(extraction);
  try {
    const platforms = await discoverPlatformInputs(join(dist, "npm-inputs"), extraction);
    // Warm the npm shim so proto diagnostics cannot corrupt npm pack's JSON output.
    await commandText("npm", ["--version"]);
    await processPlugin.createDprintOrgNpmPackages({
      pluginName: PLUGIN_NAME,
      version,
      mainPackageName: MAIN_PACKAGE_NAME,
      outDir: output,
      platforms: platforms.map(({ platform, directory, executable }) => ({
        platform,
        binaryPath: join(directory, executable),
        packageContents: directory,
      })),
      packageJsonExtra: {
        description: "Oxfmt process plugin for dprint",
        license: "MIT",
        repository: {
          type: "git",
          url: "git+https://github.com/muyu258/dprint-process-plugin-oxfmt.git",
        },
        engines: { node: ">=22.12.0" },
      },
    });
    await replaceDirectory(output, join(dist, "npm"));
    console.log(`Created npm release set in ${join(dist, "npm")}`);
  } finally {
    await Deno.remove(stage, { recursive: true }).catch(() => undefined);
  }
}

if (import.meta.main) await createNpmPackages();
