import { readFile, readdir, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const uiDirectory = dirname(scriptsDirectory);
const repositoryRoot = dirname(dirname(uiDirectory));
const assetPath = join(
  repositoryRoot,
  "crates",
  "budna-mcp-server",
  "assets",
  "marketplace-explorer-v1.html",
);
const { size } = await stat(assetPath);
const maximumBytes = 300 * 1024;

if (size > maximumBytes) {
  throw new Error(
    `Embedded MCP App is ${size} bytes; the maximum is ${maximumBytes} bytes`,
  );
}

const assetsDirectory = dirname(assetPath);
const assetNames = await readdir(assetsDirectory);
const expectedAssetNames = new Set([
  "marketplace-explorer-v1.html",
  "THIRD_PARTY_NOTICES.txt",
]);
const unexpectedAssets = assetNames.filter(
  (name) => !expectedAssetNames.has(name),
);
if (unexpectedAssets.length > 0) {
  throw new Error(
    `Unexpected MCP App assets: ${unexpectedAssets.sort().join(", ")}`,
  );
}

const html = await readFile(assetPath, "utf8");
const originsMarker = "__BUDNA_MCP_PUBLIC_ORIGINS_JSON__";
const markerCount = html.split(originsMarker).length - 1;
const moduleScriptIndex = html.search(/<script\b[^>]*\btype=["']module["']/iu);
const markerIndex = html.indexOf(originsMarker);
if (
  markerCount !== 1 ||
  moduleScriptIndex < 0 ||
  markerIndex > moduleScriptIndex
) {
  throw new Error(
    "MCP App must contain one public-origins JSON marker before its module script",
  );
}
const forbiddenPatterns = [
  {
    label: "direct network request",
    pattern: /\b(?:fetch|WebSocket)\s*\(|XMLHttpRequest/u,
  },
  {
    label: "dynamic HTML injection",
    pattern: /\b(?:innerHTML|insertAdjacentHTML|outerHTML)\b/u,
  },
  {
    label: "browser storage or cookie access",
    pattern: /\b(?:localStorage|sessionStorage|document\.cookie)\b/u,
  },
  {
    label: "unapproved browser egress",
    pattern: /\b(?:navigator\.sendBeacon|window\.open)\s*\(/u,
  },
  { label: "external script", pattern: /<script\b[^>]*\bsrc\s*=/iu },
  { label: "external stylesheet", pattern: /<link\b[^>]*\bhref\s*=/iu },
  { label: "source map reference", pattern: /sourceMappingURL/iu },
];
for (const { label, pattern } of forbiddenPatterns) {
  if (pattern.test(html))
    throw new Error(`MCP App contains a forbidden ${label}`);
}

const noticePath = join(assetsDirectory, "THIRD_PARTY_NOTICES.txt");
const notice = await readFile(noticePath, "utf8");
const packageManifest = JSON.parse(
  await readFile(join(uiDirectory, "package.json"), "utf8"),
);
const packageLock = JSON.parse(
  await readFile(join(uiDirectory, "package-lock.json"), "utf8"),
);
const workspaceManifest = await readFile(
  join(repositoryRoot, "Cargo.toml"),
  "utf8",
);
const workspaceVersion = workspaceManifest.match(
  /\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/u,
)?.[1];
const bridgeSource = await readFile(
  join(uiDirectory, "src", "bridge.ts"),
  "utf8",
);
const bridgeVersion = bridgeSource.match(
  /appInfo:\s*\{[^}]*version:\s*"([^"]+)"/u,
)?.[1];
for (const [surface, version] of [
  ["npm lockfile", packageLock.packages?.[""]?.version],
  ["Rust workspace", workspaceVersion],
  ["MCP App bridge", bridgeVersion],
]) {
  if (version !== packageManifest.version) {
    throw new Error(
      `${surface} version ${String(version)} does not match UI version ${packageManifest.version}`,
    );
  }
}
const directPackages = {
  ...packageManifest.dependencies,
  ...packageManifest.devDependencies,
};
for (const [name, version] of Object.entries(directPackages)) {
  if (!notice.includes(`${name} ${version}`)) {
    throw new Error(`Third-party notice is missing ${name} ${version}`);
  }
}

process.stdout.write(`Embedded MCP App: ${size} bytes\n`);
