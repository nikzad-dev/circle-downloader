import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");

console.log("Generating SPDX 2.3 Software Bill of Materials (SBOM)...");

// 1. Collect Rust dependencies via cargo metadata
console.log("Analyzing Rust dependencies via Cargo metadata...");
let rustPackages = [];
try {
  const metadataRaw = execSync(
    "cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1",
    { cwd: rootDir, maxBuffer: 30 * 1024 * 1024 },
  ).toString();
  const metadata = JSON.parse(metadataRaw);

  rustPackages = metadata.packages.map((pkg) => ({
    name: pkg.name,
    version: pkg.version,
    license: pkg.license || "NOASSERTION",
    repository: pkg.repository || null,
    source: pkg.source || "workspace",
    type: "rust",
  }));
} catch (err) {
  console.warn("Warning: Could not fetch cargo metadata:", err.message);
}

// 2. Collect npm dependencies
console.log("Analyzing npm direct dependencies from package.json and node_modules...");
const pkgJson = JSON.parse(fs.readFileSync(path.join(rootDir, "package.json"), "utf8"));
const npmDeps = {
  ...pkgJson.dependencies,
  ...pkgJson.devDependencies,
};

const npmPackages = [];
for (const [depName, declaredVersion] of Object.entries(npmDeps)) {
  const depPkgPath = path.join(rootDir, "node_modules", depName, "package.json");
  let license = "NOASSERTION";
  let resolvedVersion = declaredVersion;
  let repo = null;

  if (fs.existsSync(depPkgPath)) {
    try {
      const depPkg = JSON.parse(fs.readFileSync(depPkgPath, "utf8"));
      license = depPkg.license || "NOASSERTION";
      resolvedVersion = depPkg.version || declaredVersion;
      if (typeof depPkg.repository === "string") {
        repo = depPkg.repository;
      } else if (depPkg.repository?.url) {
        repo = depPkg.repository.url;
      }
    } catch {
      // Ignore read errors
    }
  }

  npmPackages.push({
    name: depName,
    version: resolvedVersion,
    license: typeof license === "string" ? license : JSON.stringify(license),
    repository: repo,
    source: "npm",
    type: "npm",
  });
}

// 3. Assemble SPDX 2.3 structure
const now = new Date().toISOString();
const spdxDocument = {
  spdxVersion: "SPDX-2.3",
  dataLicense: "CC0-1.0",
  SPDXID: "SPDXRef-DOCUMENT",
  name: "circle-downloader-sbom",
  documentNamespace: `https://github.com/nikzad-dev/circle-downloader/spdx/${Date.now()}`,
  creationInfo: {
    creators: [
      "Tool: circle-downloader-sbom-generator-1.0",
      "Organization: Circle Downloader Maintainers",
    ],
    created: now,
  },
  packages: [
    {
      name: "circle-downloader",
      SPDXID: "SPDXRef-Package-circle-downloader",
      versionInfo: pkgJson.version || "0.1.0",
      packageSupplier: "Organization: Circle Downloader",
      downloadLocation: "https://github.com/nikzad-dev/circle-downloader",
      filesAnalyzed: false,
      licenseConcluded: "GPL-3.0-or-later",
      licenseDeclared: "GPL-3.0-or-later",
      copyrightText: "Copyright (c) 2026 Circle Downloader contributors",
      description: "Local desktop download manager built with Tauri, Rust and React",
    },
    ...rustPackages.map((pkg, idx) => ({
      name: pkg.name,
      SPDXID: `SPDXRef-Package-rust-${idx}-${pkg.name.replace(/[^a-zA-Z0-9.-]/g, "-")}`,
      versionInfo: pkg.version,
      packageSupplier: "NOASSERTION",
      downloadLocation: pkg.repository || "NOASSERTION",
      filesAnalyzed: false,
      licenseConcluded: pkg.license,
      licenseDeclared: pkg.license,
      copyrightText: "NOASSERTION",
      comment: `Ecosystem: Cargo | Source: ${pkg.source}`,
    })),
    ...npmPackages.map((pkg, idx) => ({
      name: pkg.name,
      SPDXID: `SPDXRef-Package-npm-${idx}-${pkg.name.replace(/[^a-zA-Z0-9.-]/g, "-")}`,
      versionInfo: pkg.version,
      packageSupplier: "NOASSERTION",
      downloadLocation: pkg.repository || "NOASSERTION",
      filesAnalyzed: false,
      licenseConcluded: pkg.license,
      licenseDeclared: pkg.license,
      copyrightText: "NOASSERTION",
      comment: `Ecosystem: npm`,
    })),
  ],
  relationships: [
    ...rustPackages.map((pkg, idx) => ({
      spdxElementId: "SPDXRef-Package-circle-downloader",
      relationshipType: "DEPENDS_ON",
      relatedSpdxElement: `SPDXRef-Package-rust-${idx}-${pkg.name.replace(/[^a-zA-Z0-9.-]/g, "-")}`,
    })),
    ...npmPackages.map((pkg, idx) => ({
      spdxElementId: "SPDXRef-Package-circle-downloader",
      relationshipType: "DEPENDS_ON",
      relatedSpdxElement: `SPDXRef-Package-npm-${idx}-${pkg.name.replace(/[^a-zA-Z0-9.-]/g, "-")}`,
    })),
  ],
};

const outputDir = path.join(rootDir, "docs", "licenses");
if (!fs.existsSync(outputDir)) {
  fs.mkdirSync(outputDir, { recursive: true });
}

const outputPath = path.join(outputDir, "sbom.spdx.json");
fs.writeFileSync(outputPath, JSON.stringify(spdxDocument, null, 2), "utf8");

console.log(`Successfully generated SPDX 2.3 SBOM at: ${outputPath}`);
console.log(
  `Inventoried ${rustPackages.length} Rust crates and ${npmPackages.length} npm packages.`,
);
