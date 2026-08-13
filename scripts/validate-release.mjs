import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { analyzeCommits } from "@semantic-release/commit-analyzer";
import semver from "semver";
import { parse } from "yaml";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url);

function run(command, args) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  });
}

function fail(message) {
  throw new Error(message);
}

function pluginName(plugin) {
  return Array.isArray(plugin) ? plugin[0] : plugin;
}

function findPlugin(plugins, name) {
  const plugin = plugins.find((entry) => pluginName(entry) === name);
  if (!plugin) {
    fail(`missing Semantic Release plugin: ${name}`);
  }
  return plugin;
}

function latestReleaseTag() {
  const releases = run("git", ["tag", "--list", "v*"])
    .split("\n")
    .filter(Boolean)
    .map((tag) => ({ tag, version: semver.valid(tag.slice(1)) }))
    .filter(({ version }) => version && semver.prerelease(version) === null)
    .sort((left, right) => semver.rcompare(left.version, right.version));

  if (releases.length === 0) {
    fail("no stable v-prefixed release tag found");
  }
  return releases[0];
}

function workspacePackages() {
  const metadata = JSON.parse(
    run("cargo", ["metadata", "--no-deps", "--format-version", "1", "--locked"]),
  );
  const members = new Set(metadata.workspace_members);
  return metadata.packages.filter(({ id }) => members.has(id));
}

function validateWorkspaceVersions(packages, releaseVersion) {
  const packageDirectories = new Set(
    packages.map(({ manifest_path: manifestPath }) => dirname(manifestPath)),
  );

  for (const packageMetadata of packages) {
    if (packageMetadata.version !== releaseVersion) {
      fail(
        `${packageMetadata.name} is ${packageMetadata.version}; keep it at ${releaseVersion} until Semantic Release runs`,
      );
    }

    for (const dependency of packageMetadata.dependencies) {
      if (!dependency.path || !packageDirectories.has(resolve(dependency.path))) {
        continue;
      }
      const minimum = semver.minVersion(dependency.req);
      if (!minimum || minimum.version !== releaseVersion) {
        fail(
          `${packageMetadata.name} requires ${dependency.name} ${dependency.req}; expected ${releaseVersion}`,
        );
      }
    }
  }
}

function commitsSince(tag) {
  const fields = run("git", ["log", "-z", "--format=%H%x00%B", `${tag}..HEAD`]).split("\0");
  if (fields.at(-1) === "") {
    fields.pop();
  }
  if (fields.length % 2 !== 0) {
    fail("could not read Git commit history");
  }

  const commits = [];
  for (let index = 0; index < fields.length; index += 2) {
    commits.push({ hash: fields[index].trim(), message: fields[index + 1].trimEnd() });
  }
  return commits;
}

async function validate() {
  const config = parse(await readFile(resolve(root, ".releaserc.yml"), "utf8"));
  if (config.tagFormat !== "v${version}") {
    fail('tagFormat must be "v${version}"');
  }
  if (!Array.isArray(config.plugins)) {
    fail("Semantic Release plugins must be a list");
  }

  for (const plugin of config.plugins) {
    require.resolve(pluginName(plugin));
  }

  const execPlugin = findPlugin(config.plugins, "@semantic-release/exec");
  const execOptions = Array.isArray(execPlugin) ? execPlugin[1] : {};
  const prepareCommand = execOptions?.prepareCmd;
  const expectedPrepareCommand = "./scripts/bump-version.sh ${nextRelease.version}";
  if (prepareCommand !== expectedPrepareCommand) {
    fail(`prepareCmd must be ${expectedPrepareCommand}`);
  }

  const { tag, version } = latestReleaseTag();
  validateWorkspaceVersions(workspacePackages(), version);

  const analyzerPlugin = findPlugin(config.plugins, "@semantic-release/commit-analyzer");
  const analyzerOptions = Array.isArray(analyzerPlugin) ? analyzerPlugin[1] : {};
  const commits = commitsSince(tag);
  const releaseType = await analyzeCommits(analyzerOptions, {
    commits,
    cwd: root,
    logger: { log() {} },
  });

  console.log(`Current release: ${tag}`);
  if (releaseType) {
    console.log(`Pending release: ${releaseType} -> v${semver.inc(version, releaseType)}`);
  } else {
    console.log("Pending release: none");
  }
  console.log("Release validation passed");
}

validate().catch((error) => {
  console.error(`Release validation failed: ${error.message}`);
  process.exitCode = 1;
});
