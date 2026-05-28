#!/usr/bin/env node
const { spawnSync } = require("child_process");
const path = require("path");

function getBinaryPath() {
  const platform = process.platform;
  const arch = process.arch;

  const mapping = {
    "darwin arm64": "openpage-bin-darwin-arm64",
    "darwin x64": "openpage-bin-darwin-x64",
    "linux x64": "openpage-bin-linux-x64-gnu",
    "linux arm64": "openpage-bin-linux-arm64-gnu",
    "win32 x64": "openpage-bin-win32-x64-msvc"
  };

  const key = `${platform} ${arch}`;
  const pkgName = mapping[key];

  if (!pkgName) {
    console.error(`Unsupported platform: ${platform} ${arch}`);
    process.exit(1);
  }

  try {
    const pkgPath = require.resolve(`${pkgName}/package.json`);
    const pkgDir = path.dirname(pkgPath);
    const binaryName = platform === "win32" ? "openpage.exe" : "openpage";
    return path.join(pkgDir, "bin", binaryName);
  } catch (err) {
    console.error(`Platform package not found: ${pkgName}`);
    console.error("Please reinstall openpage or build from source.");
    process.exit(1);
  }
}

const binaryPath = getBinaryPath();
const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  shell: false,
  env: {
    ...process.env,
    OPENPAGE_INSTALL_SOURCE: "npm"
  }
});

if (result.error) {
  console.error(`Failed to run openpage: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 0);
