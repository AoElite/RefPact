# Stub packages

Tiny stand-ins for npm packages. The fixtures depend on them through the pnpm workspace, so
they are found in `node_modules` like real packages, without network access.

- `fakepkg` has an `exports` map with a root entry and a subpath.
- `@fixture/scoped` is a scoped package.
