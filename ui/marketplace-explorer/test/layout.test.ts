import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const testDirectory = dirname(fileURLToPath(import.meta.url));
const uiDirectory = dirname(testDirectory);
const stylesheet = readFileSync(join(uiDirectory, "src", "style.css"), "utf8");

describe("responsive and motion-safe presentation", () => {
  it("defines compact mobile layouts and a reduced-motion mode", () => {
    expect(stylesheet).toContain("@media (max-width: 620px)");
    expect(stylesheet).toContain("grid-template-columns: 1fr");
    expect(stylesheet).toContain("@media (prefers-reduced-motion: reduce)");
  });
});
