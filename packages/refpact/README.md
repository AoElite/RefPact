# refpact

Architecture tests for TypeScript 6 and 7: rules about what files may import, checked as
ordinary tests under Vitest or `node --test`. The compiler of your project resolves each
import.

```ts
import { test } from "vitest";
import { anyOf, loadProject, pkg, rule } from "refpact";

const app = loadProject("tsconfig.json");

test("the user interface does not import the database", () => {
  rule("the user interface does not import the database")
    .files(anyOf("src/ui/**"))
    .mustNotImport(anyOf("src/db/**", pkg("pg")))
    .check(app);
});
```

`check` throws an error, and the message of the error is the report:

```text
rule `the user interface does not import the database` is violated in 1 place(s):
  src/ui/view.ts:3:10: [the user interface does not import the database] src/ui/view.ts uses src/db/client.ts#query (import, written as src/db/index.ts#query)
```

- [The TypeScript guide](https://github.com/AoElite/RefPact/blob/main/docs/typescript.md):
  how to install the package, every rule, and the limits of the analysis.
- [All that you can check](https://github.com/AoElite/RefPact#what-you-can-check), also in
  Rust.

## License

MIT
