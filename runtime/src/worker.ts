import { once } from "node:events";
import { realpathSync } from "node:fs";
import { createInterface } from "node:readline";
import type { Readable, Writable } from "node:stream";
import { fileURLToPath } from "node:url";

import { format, type FormatConfig } from "oxfmt";

type FormatRequest = {
  fileName: string;
  sourceText: string;
  options: FormatConfig;
};

export async function runWorker(
  input: Readable = process.stdin,
  output: Writable = process.stdout,
): Promise<void> {
  const lines = createInterface({ input, crlfDelay: Number.POSITIVE_INFINITY });

  for await (const line of lines) {
    const request = JSON.parse(line) as FormatRequest;
    try {
      await writeMessage(
        output,
        await format(request.fileName, request.sourceText, request.options),
      );
    } catch (error) {
      await writeMessage(output, { error: errorMessage(error) });
    }
  }
}

async function writeMessage(output: Writable, message: unknown): Promise<void> {
  if (!output.write(`${JSON.stringify(message)}\n`)) {
    await once(output, "drain");
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const entryPath = process.argv[1];
if (
  entryPath !== undefined &&
  realpathSync(fileURLToPath(import.meta.url)) === realpathSync(entryPath)
) {
  runWorker().catch(async (error: unknown) => {
    await writeMessage(process.stdout, { error: errorMessage(error) });
    process.exitCode = 1;
  });
}
