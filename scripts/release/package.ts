import { processPlugin } from "@dprint/automation";
import { basename, join } from "@std/path";
import {
  assertReleaseVersions,
  commandText,
  PLUGIN_NAME,
  replaceDirectory,
  repositoryRoot,
  run,
} from "./common.ts";

/**
 * Builds the release ZIP for the host platform.
 */
async function buildPlatformPackage(): Promise<void> {
  await assertReleaseVersions();

  const platform = processPlugin.getCurrentPlatform();
  const executableName = `${PLUGIN_NAME}${Deno.build.os === "windows" ? ".exe" : ""}`;
  const zipName = processPlugin.getStandardZipFileName(PLUGIN_NAME, platform);

  const dist = join(repositoryRoot, "dist");
  await Deno.mkdir(dist, { recursive: true });
  const stage = await Deno.makeTempDir({ dir: dist, prefix: ".platform-stage-" });
  const payload = join(stage, "payload");
  const stagedPlatform = join(stage, platform);
  await Deno.mkdir(join(payload, "runtime/dist"), { recursive: true });
  await Deno.mkdir(stagedPlatform);

  try {
    // Build both halves of the process plugin: the Rust protocol executable and the Node worker.
    // The frozen installs ensure the files copied below match the versions in deno.lock.
    await run("cargo", [
      "build",
      "--manifest-path",
      join(repositoryRoot, "Cargo.toml"),
      "--release",
    ]);
    await run("deno", ["install", "--frozen", "--node-modules-linker=hoisted"], {
      cwd: repositoryRoot,
    });
    await run("deno", ["task", "runtime:build"], { cwd: repositoryRoot });

    const metadata = JSON.parse(
      await commandText("cargo", [
        "metadata",
        "--manifest-path",
        join(repositoryRoot, "Cargo.toml"),
        "--format-version",
        "1",
        "--no-deps",
      ]),
    ) as { target_directory: string };
    await Deno.copyFile(
      join(metadata.target_directory, "release", executableName),
      join(payload, executableName),
    );
    if (Deno.build.os !== "windows") await Deno.chmod(join(payload, executableName), 0o755);

    await Deno.copyFile(
      join(repositoryRoot, "runtime/package.json"),
      join(payload, "runtime/package.json"),
    );
    await Deno.copyFile(
      join(repositoryRoot, "runtime/dist/worker.js"),
      join(payload, "runtime/dist/worker.js"),
    );

    await run(
      "deno",
      [
        "install",
        "--no-config",
        "--package-json",
        "--prod",
        "--frozen",
        "--lock",
        join(repositoryRoot, "deno.lock"),
        "--node-modules-dir=manual",
        "--node-modules-linker=hoisted",
      ],
      { cwd: join(payload, "runtime") },
    );

    // zip runs from payload so payload itself is not added as an extra top-level directory.
    const zipPath = join(stagedPlatform, zipName);
    await run("zip", ["-q", "-r", zipPath, executableName, "runtime"], { cwd: payload });
    const destinationRoot = join(dist, "npm-inputs");
    await Deno.mkdir(destinationRoot, { recursive: true });

    // Only expose the new platform output after the ZIP has been created successfully.
    await replaceDirectory(stagedPlatform, join(destinationRoot, platform));

    // The release workflow uses these values to name and upload the platform artifact.
    const githubOutput = Deno.env.get("GITHUB_OUTPUT");
    if (githubOutput != null) {
      await Deno.writeTextFile(
        githubOutput,
        `platform=${platform}\nzip_name=${basename(zipPath)}\n`,
        {
          append: true,
        },
      );
    }
    console.log(`Created dist/npm-inputs/${platform}/${zipName}`);
  } finally {
    // Clean up extracted files and failed intermediate output without hiding the original error.
    await Deno.remove(stage, { recursive: true }).catch(() => undefined);
  }
}

if (import.meta.main) await buildPlatformPackage();
