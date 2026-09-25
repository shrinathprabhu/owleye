import { spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import {
  prerequisites,
  root,
  lock,
  run,
  config,
  build,
  deploy,
} from "./common.mjs";
try {
  const git = (...args) =>
    spawnSync("git", args, { cwd: root, encoding: "utf8" });
  const top = git("rev-parse", "--show-toplevel");
  if (
    top.status !== 0 ||
    realpathSync(top.stdout.trim()) !== realpathSync(root)
  )
    throw new Error(
      "This installation is not a Git checkout. Upload the updated files manually and run ./redeploy.sh.",
    );
  if (git("status", "--porcelain").stdout.trim())
    throw new Error(
      "Your checkout has local changes. Commit or stash them before updating.",
    );
  if (git("symbolic-ref", "-q", "HEAD").status !== 0)
    throw new Error(
      "Detached checkout: switch to your tracked branch before updating.",
    );
  if (git("rev-parse", "--abbrev-ref", "@{upstream}").status !== 0)
    throw new Error(
      "No upstream branch configured. Configure one before updating.",
    );
  await prerequisites();
  await lock(async () => {
    await run("git", ["pull", "--ff-only"]);
    const settings = await config();
    const release = await build();
    await deploy(settings, release);
  });
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
