# Guidelines

Follow these when you design a change.

- **Built-in rules are custom rules.** Everything the project ships uses the same check
  interface that users have. When a built-in needs something the interface lacks, extend
  the interface first, then build on it.
- **The public API stays minimal.** Add to it only when nothing existing can express
  the need. A second way to do the same thing is allowed only when the first is
  measurably too slow, and the code says so.
- **One implementation, used everywhere.** Each behaviour is written once and shared
  by everything that has it. Look for the existing piece and extend it before
  writing a new one.
- **Every supported language has the same features.** A feature has the same name,
  meaning and documentation in each language. Where a language makes that impossible,
  the documentation says so.
- **Slow analysis is opt-in.** Optimize first. If it is still slow, put it behind an
  option that is off by default.
- **A rule that cannot fail is an error.** A pattern, filter or exemption that matches
  nothing is reported, never silently passed.
- **Stay in scope.** The project checks architecture: what depends on what, and where
  things live. Leave to the linters what a linter already covers.
- **Tests are few and broad.** Cover the most behaviour with the fewest tests. A test
  may exercise several features together through the public API. Add the case to an
  existing test first; write a new test only when the case is important enough for
  its own test.
- **Add a test for each bug.** The test fails without the fix, so the same bug is
  caught if it comes back.
