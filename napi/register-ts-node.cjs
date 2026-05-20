// Loading `ts-node/register` directly via AVA's `require` config causes every
// test worker to fall through AVA's `loadRequiredModule` into `importFromProject`,
// which races to `writeFileAtomic` the shared
// `node_modules/.cache/ava/import-from-project.mjs` stub. On Windows that
// concurrent rename fails intermittently with:
//   EPERM: operation not permitted, rename '...import-from-project.mjs.<pid>' -> '...import-from-project.mjs'
// (see npm/write-file-atomic#28 / #227).
//
// Pointing AVA at this file (a real path relative to projectDir) makes
// `require(fullPath)` succeed on the first try, so AVA never writes the
// stub and the race goes away.
require("ts-node/register");
