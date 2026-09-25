import { prerequisites, lock, config, build, deploy } from "./common.mjs";
try {
  await prerequisites();
  await lock(async () => {
    const settings = await config();
    const release = await build();
    await deploy(settings, release);
  });
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
