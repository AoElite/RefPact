# Stub crates

Tiny stand-ins for `rand`, `rand_core`, `getrandom` and two invented crates (`logkit`,
`tracekit`). They have the names and items that the fixtures refer to, and nothing else, so
that the fixture crates compile without network access.

`facade` and `engine` are two more invented crates: `facade` gives the items of `engine` again,
for the fixture `definitions`.

`engine-next` is the package `engine` in a later version, for the fixture `packages`: a build
of that workspace needs two versions of one package. Some stubs name a licence, for the same
fixture.
