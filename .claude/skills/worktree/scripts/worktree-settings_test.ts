import { assertArrayIncludes, assertEquals } from "@std/assert";

/** Helper: create a temp directory structure with optional files. */
function makeTempWorktree(
  files: Record<string, unknown> = {},
): string {
  const dir = Deno.makeTempDirSync();
  for (const [relPath, content] of Object.entries(files)) {
    const fullPath = `${dir}/${relPath}`;
    const parentDir = fullPath.substring(0, fullPath.lastIndexOf("/"));
    Deno.mkdirSync(parentDir, { recursive: true });
    Deno.writeTextFileSync(fullPath, JSON.stringify(content, null, 2) + "\n");
  }
  return dir;
}

function readJson(path: string): Record<string, unknown> {
  return JSON.parse(Deno.readTextFileSync(path));
}

/** Run the script as a subprocess. */
async function runScript(
  ...args: string[]
): Promise<{ code: number; stdout: string; stderr: string }> {
  const scriptPath = new URL("./worktree-settings.ts", import.meta.url)
    .pathname;
  const cmd = new Deno.Command("deno", {
    args: [
      "run",
      "--allow-read",
      "--allow-write",
      scriptPath,
      ...args,
    ],
    stdout: "piped",
    stderr: "piped",
  });
  const output = await cmd.output();
  return {
    code: output.code,
    stdout: new TextDecoder().decode(output.stdout),
    stderr: new TextDecoder().decode(output.stderr),
  };
}

Deno.test("setup: copies settings.local.json to new worktree (same parent)", async () => {
  const parent = Deno.makeTempDirSync();
  const mainWt = `${parent}/mementor`;
  const newWt = `${parent}/mementor-feature`;
  Deno.mkdirSync(`${mainWt}/.claude`, { recursive: true });
  Deno.mkdirSync(newWt, { recursive: true });

  const localSettings = {
    permissions: { allow: ["Bash(git commit:*)", "Bash(git push:*)"] },
    enabledPlugins: { "feature-dev@claude-plugins-official": true },
  };
  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.local.json`,
    JSON.stringify(localSettings, null, 2) + "\n",
  );
  // Need settings.json for -C generation
  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.json`,
    JSON.stringify({ permissions: { allow: [] } }, null, 2) + "\n",
  );

  const result = await runScript("setup", mainWt, newWt);
  assertEquals(result.code, 0, result.stderr);

  const copied = readJson(`${newWt}/.claude/settings.local.json`);
  assertEquals(
    (copied.permissions as { allow: string[] }).allow,
    localSettings.permissions.allow,
  );
  assertEquals(copied.enabledPlugins, localSettings.enabledPlugins);
});

Deno.test("setup: rewrites paths when parents differ", async () => {
  const oldParent = Deno.makeTempDirSync();
  const newParent = Deno.makeTempDirSync();
  const mainWt = `${oldParent}/mementor`;
  const newWt = `${newParent}/mementor-feature`;
  Deno.mkdirSync(`${mainWt}/.claude`, { recursive: true });
  Deno.mkdirSync(newWt, { recursive: true });

  const localSettings = {
    permissions: {
      allow: [
        `Bash(git -C ${oldParent}/mementor commit *)`,
        "Bash(git push:*)",
      ],
    },
  };
  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.local.json`,
    JSON.stringify(localSettings, null, 2) + "\n",
  );
  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.json`,
    JSON.stringify({ permissions: { allow: [] } }, null, 2) + "\n",
  );

  const result = await runScript("setup", mainWt, newWt);
  assertEquals(result.code, 0, result.stderr);

  const copied = readJson(`${newWt}/.claude/settings.local.json`);
  const allow = (copied.permissions as { allow: string[] }).allow;
  assertEquals(allow[0], `Bash(git -C ${newParent}/mementor commit *)`);
  assertEquals(allow[1], "Bash(git push:*)");
});

Deno.test("setup: generates -C entries in main's settings.local.json", async () => {
  const parent = Deno.makeTempDirSync();
  const mainWt = `${parent}/mementor`;
  const newWt = `${parent}/mementor-feature`;
  Deno.mkdirSync(`${mainWt}/.claude`, { recursive: true });
  Deno.mkdirSync(newWt, { recursive: true });

  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.json`,
    JSON.stringify(
      {
        permissions: { allow: ["Bash(git add *)", "Bash(git fetch *)"] },
      },
      null,
      2,
    ) + "\n",
  );
  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.local.json`,
    JSON.stringify(
      {
        permissions: { allow: ["Bash(git commit:*)"] },
      },
      null,
      2,
    ) + "\n",
  );

  const result = await runScript("setup", mainWt, newWt);
  assertEquals(result.code, 0, result.stderr);

  const mainLocal = readJson(`${mainWt}/.claude/settings.local.json`);
  const allow = (mainLocal.permissions as { allow: string[] }).allow;
  assertArrayIncludes(allow, [
    `Bash(git -C ${newWt} add *)`,
    `Bash(git -C ${newWt} fetch *)`,
    `Bash(git -C ${newWt} commit *)`,
  ]);
});

Deno.test("setup: skips existing -C entries (no duplicates)", async () => {
  const parent = Deno.makeTempDirSync();
  const mainWt = `${parent}/mementor`;
  const newWt = `${parent}/mementor-feature`;
  Deno.mkdirSync(`${mainWt}/.claude`, { recursive: true });
  Deno.mkdirSync(newWt, { recursive: true });

  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.json`,
    JSON.stringify(
      {
        permissions: { allow: ["Bash(git add *)"] },
      },
      null,
      2,
    ) + "\n",
  );
  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.local.json`,
    JSON.stringify({ permissions: { allow: [] } }, null, 2) + "\n",
  );

  // Run twice
  await runScript("setup", mainWt, newWt);
  const result = await runScript("setup", mainWt, newWt);
  assertEquals(result.code, 0, result.stderr);

  const mainLocal = readJson(`${mainWt}/.claude/settings.local.json`);
  const allow = (mainLocal.permissions as { allow: string[] }).allow;
  const cEntries = allow.filter((r: string) =>
    r.includes(`git -C ${newWt} add`)
  );
  assertEquals(cEntries.length, 1, "Should have exactly one -C entry");
});

Deno.test("setup: handles missing settings.local.json gracefully", async () => {
  const parent = Deno.makeTempDirSync();
  const mainWt = `${parent}/mementor`;
  const newWt = `${parent}/mementor-feature`;
  Deno.mkdirSync(`${mainWt}/.claude`, { recursive: true });
  Deno.mkdirSync(newWt, { recursive: true });
  // No settings.local.json, only settings.json
  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.json`,
    JSON.stringify({ permissions: { allow: ["Bash(git add *)"] } }, null, 2) +
      "\n",
  );

  const result = await runScript("setup", mainWt, newWt);
  assertEquals(result.code, 0, result.stderr);

  // New worktree should NOT have settings.local.json
  let exists = true;
  try {
    Deno.statSync(`${newWt}/.claude/settings.local.json`);
  } catch {
    exists = false;
  }
  assertEquals(exists, false);
});

Deno.test("cleanup: removes -C entries for removed worktree", async () => {
  const parent = Deno.makeTempDirSync();
  const mainWt = `${parent}/mementor`;
  const removedWt = `${parent}/mementor-feature`;
  const otherWt = `${parent}/mementor-other`;
  Deno.mkdirSync(`${mainWt}/.claude`, { recursive: true });
  Deno.mkdirSync(`${removedWt}/.claude`, { recursive: true });

  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.local.json`,
    JSON.stringify(
      {
        permissions: {
          allow: [
            "Bash(git commit:*)",
            `Bash(git -C ${removedWt} add *)`,
            `Bash(git -C ${removedWt} commit *)`,
            `Bash(git -C ${otherWt} add *)`,
          ],
        },
      },
      null,
      2,
    ) + "\n",
  );

  const result = await runScript("cleanup", mainWt, removedWt);
  assertEquals(result.code, 0, result.stderr);

  const mainLocal = readJson(`${mainWt}/.claude/settings.local.json`);
  const allow = (mainLocal.permissions as { allow: string[] }).allow;
  assertEquals(allow, [
    "Bash(git commit:*)",
    `Bash(git -C ${otherWt} add *)`,
  ]);
});

Deno.test("cleanup: merges new permissions from worktree to main", async () => {
  const parent = Deno.makeTempDirSync();
  const mainWt = `${parent}/mementor`;
  const removedWt = `${parent}/mementor-feature`;
  Deno.mkdirSync(`${mainWt}/.claude`, { recursive: true });
  Deno.mkdirSync(`${removedWt}/.claude`, { recursive: true });

  Deno.writeTextFileSync(
    `${mainWt}/.claude/settings.local.json`,
    JSON.stringify(
      {
        permissions: {
          allow: [
            "Bash(git commit:*)",
            `Bash(git -C ${removedWt} add *)`,
          ],
        },
      },
      null,
      2,
    ) + "\n",
  );
  Deno.writeTextFileSync(
    `${removedWt}/.claude/settings.local.json`,
    JSON.stringify(
      {
        permissions: {
          allow: [
            "Bash(git commit:*)",
            "Bash(npm:*)",
          ],
        },
      },
      null,
      2,
    ) + "\n",
  );

  const result = await runScript("cleanup", mainWt, removedWt);
  assertEquals(result.code, 0, result.stderr);

  const mainLocal = readJson(`${mainWt}/.claude/settings.local.json`);
  const allow = (mainLocal.permissions as { allow: string[] }).allow;
  // Should have: original git commit, merged npm, but NOT the -C entry
  assertArrayIncludes(allow, ["Bash(git commit:*)", "Bash(npm:*)"]);
  assertEquals(
    allow.filter((r: string) => r.includes("git -C")).length,
    0,
    "All -C entries should be removed",
  );
});
