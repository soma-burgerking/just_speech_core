import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { countScriptLines, countScriptTokens } from "./script.ts";

interface Case {
  name: string;
  script: string;
  tokens: number;
  lines: number;
}

const cases: Case[] = JSON.parse(
  readFileSync(new URL("../vectors/cases.json", import.meta.url), "utf8"),
);

for (const c of cases) {
  test(c.name, () => {
    assert.equal(countScriptTokens(c.script), c.tokens);
    assert.equal(countScriptLines(c.script), c.lines);
  });
}
