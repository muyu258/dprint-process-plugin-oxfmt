import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";
import { PassThrough } from "node:stream";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { runWorker } from "./worker.ts";

type Request = {
  fileName: string;
  sourceText: string;
  options: Record<string, unknown>;
};

class WorkerHarness {
  readonly #input = new PassThrough();
  readonly #output = new PassThrough();
  readonly #lines = createInterface({ input: this.#output });
  readonly #iterator = this.#lines[Symbol.asyncIterator]();
  readonly #worker = runWorker(this.#input, this.#output);

  async request(message: Request): Promise<Record<string, unknown>> {
    this.#input.write(`${JSON.stringify(message)}\n`);
    const result = await this.#iterator.next();
    assert.equal(result.done, false, "worker closed before responding");
    return JSON.parse(result.value) as Record<string, unknown>;
  }

  async close(): Promise<void> {
    this.#input.end();
    await this.#worker;
    this.#lines.close();
    this.#output.destroy();
  }
}

test("formats sequential requests and forwards options", async () => {
  const worker = new WorkerHarness();

  assert.deepEqual(
    await worker.request({
      fileName: "relative/example.ts",
      sourceText: 'const value="hello"\n',
      options: {},
    }),
    {
      code: 'const value = "hello";\n',
      errors: [],
    },
  );
  assert.deepEqual(
    await worker.request({
      fileName: "relative/example.ts",
      sourceText: 'const value="hello"\n',
      options: { singleQuote: true },
    }),
    {
      code: "const value = 'hello';\n",
      errors: [],
    },
  );

  await worker.close();
});

test("returns syntax diagnostics unchanged", async () => {
  const worker = new WorkerHarness();
  const result = await worker.request({
    fileName: "example.ts",
    sourceText: "const =\n",
    options: {},
  });
  const errors = result.errors as Array<Record<string, unknown>>;

  assert.equal(errors[0]?.severity, "Error");
  assert.equal(errors[0]?.message, "Unexpected token");
  assert.ok(Array.isArray(errors[0]?.labels));
  assert.ok("helpMessage" in (errors[0] ?? {}));
  assert.ok("codeframe" in (errors[0] ?? {}));

  await worker.close();
});

test("returns Oxfmt's LF output for CRLF input", async () => {
  const worker = new WorkerHarness();
  assert.deepEqual(
    await worker.request({
      fileName: "example.ts",
      sourceText: "const value=1;\r\n",
      options: {},
    }),
    { code: "const value = 1;\n", errors: [] },
  );
  await worker.close();
});

test("serializes thrown formatter failures and continues", async () => {
  const worker = new WorkerHarness();
  const failure = await worker.request({
    fileName: null as unknown as string,
    sourceText: "value",
    options: {},
  });
  assert.equal(typeof failure.error, "string");

  assert.deepEqual(
    await worker.request({
      fileName: "example.ts",
      sourceText: "const value=1\n",
      options: {},
    }),
    { code: "const value = 1;\n", errors: [] },
  );
  await worker.close();
});

test("runs over stdio and exits successfully on EOF", async () => {
  const child = spawn(process.execPath, [fileURLToPath(new URL("./worker.js", import.meta.url))], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  const lines = createInterface({ input: child.stdout });
  const iterator = lines[Symbol.asyncIterator]();
  let stderr = "";
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk: string) => {
    stderr += chunk;
  });

  child.stdin.end(
    `${JSON.stringify({
      fileName: "example.ts",
      sourceText: "const value=1\n",
      options: {},
    })}\n`,
  );
  const response = await iterator.next();
  assert.equal(response.done, false);
  assert.deepEqual(JSON.parse(response.value), {
    code: "const value = 1;\n",
    errors: [],
  });

  const [exitCode, signal] = (await once(child, "exit")) as [number | null, NodeJS.Signals | null];
  assert.equal(exitCode, 0);
  assert.equal(signal, null);
  assert.equal(stderr, "");
  lines.close();
});
