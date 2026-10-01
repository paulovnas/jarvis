# Installation fixture

`extract-0.21.1.js.gz` is a test-only compressed copy of
`dist/graph/extract.js` from the published MIT-licensed
`@nanonets/graft@0.21.1` package. Its uncompressed SHA-256 is
`b3477cb83351965386d48d41fbe79093083c084299dcec2b384b16caa8ec3d60`.

Source: <https://registry.npmjs.org/@nanonets/graft/-/graft-0.21.1.tgz>.
The upstream license is preserved in `GRAFT-LICENSE`.

The fixture lets installation, repair and damaged-package tests exercise the
same pinned artifact checks as production. It is never included in production
builds or imported as a Jarvis implementation. The real runtime downloads and
verifies the official package, then replaces only native grammar imports with
the Jarvis-owned WASM adapter.
