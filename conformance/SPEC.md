# refpact core specification

This document defines the language-neutral core. The core is the Rust crate `refpact-core`
in `crates/refpact-core`, and it must behave as written here. The Rust crate calls it
directly. Every other language calls it through the JSON of section 8: the TypeScript package
runs it as WebAssembly. The files in `cases/` are the executable form of this document. The
tests run all of them against the core directly, through its JSON, and from TypeScript.

The core does not know the syntax of any language. A front end turns source into a *fact set*.
A binding builds *rules*. The core evaluates one rule against one fact set and returns either
a list of *violations* or a list of *errors*.

## 1. Paths

A path is a non-empty list of non-empty text segments. `crate::codegen::Generator` is the path
`["crate", "codegen", "Generator"]` when the separator is `::`.

The separator is only a display and input convenience. The core compares segment lists.

Path `A` is a *prefix* of path `B` when `B` starts with all segments of `A`. A path is a prefix
of itself.

## 2. Patterns

A pattern is a non-empty list of pattern segments. A pattern segment is one of:

- `**`, which matches zero or more whole path segments.
- A glob. In a glob, `*` matches any run of characters inside one segment, including an empty
  run. All other characters match themselves. Matching is case-sensitive.

A pattern is invalid when it has no segments, when a segment is empty, or when a segment
contains `**` together with other characters (for example `a**`).

Two match relations exist:

- **Exact match.** The pattern matches the whole path.
- **Match within.** The pattern matches some prefix of the path. This means: the path is the
  thing the pattern names, or it is inside that thing.

Subjects use exact match. Targets use match within. Thus the target `std::fs` covers
`std::fs::read`, but the subject `crate::interpreter` selects only that one unit, and
`crate::interpreter::**` selects it and all units below it.

## 3. Fact set

```
FactSet {
  separator   text used to print paths
  units       paths of all units (Rust: modules, TypeScript: files)
  unit_lines  the number of lines of each unit, in the order of `units`; may be empty
  items       paths of all known named things inside units
  kinds       all reference kinds this front end can produce
  markers     all markers this front end can produce
  tags        all tags this front end can produce
  facts       ordered list of facts
  declarations   ordered list of declarations
  item_kinds     all item kinds this front end can produce
  visibilities   all visibilities this front end can produce
  relations      all relations this front end can produce
  modifiers      all modifiers of declarations this front end can produce
  properties     the names of all properties of references this front end can produce
  attached_prefixes   segments that reports print without a separator, by prefix
  hidden_prefixes     prefixes that reports leave out of the first segment
}

Fact {
  unit        path of the unit that contains the fact
  file, line, column
  tags        list of tags, may be empty
  note        optional text, copied to the violation
  item        optional path of the innermost declaration that contains the fact
  one of:
    reference { target, aliases, kind, certainty, properties }
    marker    text
}
```

- `target` is the path the code refers to, resolved as far as the front end can.
- `aliases` are further paths that the same reference denotes, for example the definition
  behind a re-export. Order matters.
- `certainty` is `exact` or `candidate`. A candidate is a path the front end could not prove,
  for example a name that may come from a glob import.
- `properties` are what the front end knows of the target besides its path: a name and a
  value each, such as the version of a package. Each name is one of the `properties` of the
  fact set. No rule of this specification reads them. A check of the user (section 4.15)
  gets them with the fact, and its rule names the properties that it reads.
- Kinds, markers and tags are opaque text to the core.
- `item` says which declaration the code of the fact belongs to, such as the function whose
  body has the reference. It is a path of an item, not necessarily of a declaration: the
  Rust front end gives `Type::method` for a function of `impl Trait for Type`. A fact outside
  every declaration, such as a `use` at the top of a module, has none. Only the rules of
  section 4.12 with a demand about uses read it.

```
Declaration {
  path        path of the declared item
  name        the name of the item as the code writes it
  unit        path of the unit that declares the item
  file, line, column
  end_line    optional line on which the item ends
  tags        list of tags, may be empty
  kind        text, such as `struct` or `function`
  visibility  text, such as `pub` or `exported`
  relations   list of { relation, target }
  modifiers   list of text, such as `async` or `documented`; may be empty
}
```

A declaration is a named item that a unit declares, such as a type or a function. A
relation ties the item to a path: `{ relation: "implements", target: path of a trait }`. The
item kind, the visibility, the relation and the modifiers are opaque text to the core, like
kinds. A modifier is what the declaration says of the item besides its kind and its
visibility, such as `async` or `documented`.

`line` and `column` are the place of the name. The *lines* of a declaration are
`end_line - line + 1`, and 1 when `end_line` is before `line`. A declaration without an
`end_line` has no lines: the front end does not record where the item ends. The *lines* of
the unit at position `n` of `units` are entry `n` of `unit_lines`. A unit at a position that
`unit_lines` does not have has no lines. Only the rules of section 4.16 read the lines.

A fact set is invalid when a fact names a unit that is not in `units`, a kind that is not in
`kinds`, a marker that is not in `markers`, or a tag that is not in `tags`. It is also invalid
when a declaration names a unit that is not in `units`, an item kind that is not in
`item_kinds`, a visibility that is not in `visibilities`, a relation that is not in
`relations`, a tag that is not in `tags`, or a modifier that is not in `modifiers`.

The *owner* of a path is the longest unit that is a prefix of the path. A path with no such
unit has no owner.

The *path format* of a fact set is how reports and baselines print a path (section 7). The
segments are joined with `separator`. A segment after the first that starts with one of
`attached_prefixes` is joined without the separator, and the first segment loses the first of
`hidden_prefixes` that it starts with. Both lists are empty unless the front end gives them.
The TypeScript front end gives `["#"]` and `["npm:"]`, with the separator `/`: the path
`["npm:lodash", "fp", "#map"]` prints as `lodash/fp#map`. Errors (section 5) print the
patterns of a rule joined with `separator` only.

## 4. Rules

```
Rule {
  name
  reason      optional text: why the rule exists
  severity    error | warning                        (default error)
  subjects    { include: all | any_of [pattern], except: none | [pattern] }
  one of:
    must_not_use      { any_of [pattern], unverified [pattern] }
    must_only_use     { any_of [pattern], unverified [pattern] }
    must_not_contain  [marker]
    must_be_acyclic   { slices: none | [pattern] }
    must_be_independent     { slices: [pattern] }
    must_be_used
    must_not_reach          { any_of [pattern], unverified [pattern] }
    must_be_reachable_from  { roots: [pattern] }
    must_be_in              { places: [pattern] }
    must_only_be_used_by    { users: [pattern] }
    must_not_exceed_lines   { lines: number }
    custom                  a check of the user (section 4.15)
    items                   { named: none | [glob], kinds: none | [item kind],
                              visibilities: none | [visibility],
                              related: [{ relation, any_of: [pattern] }],
                              modifiers: none | [modifier],
                              must: not_exist | reside_in [pattern] | be_named [glob]
                                    | have { relation, any_of: [pattern] }
                                    | have_modifier [modifier]
                                    | not_use { any_of [pattern], unverified [pattern] }
                                    | only_use { any_of [pattern], unverified [pattern] }
                                    | not_exceed_lines number
                                    | custom   a check of the user (section 4.15) }
  kinds         all | only [kind] | except [kind]      (default all)
  ignore_tags   [tag]                                  (default empty)
  candidates    include | ignore                       (default include)
  exemptions    [{ from: [pattern], to: [pattern] }]   (default empty)
  properties    [name]                                 (default empty)
}
```

The `reason` does not change the evaluation. The report of the violations of a rule
(`render-violations`, section 8) prints it on the line below the heading, as
`  because <reason>`. An empty reason is not printed.

The `severity` does not change the evaluation either: a warning has the violations and the
errors of an error. The heading of the report of a warning starts with `warning: `. A
binding fails a check on the violations of an error only; it reports those of a warning and
passes. A warning with errors fails, as every rule with errors does.

### 4.1 Subjects

The selected units are the units that match at least one `include` pattern exactly (or all
units for `all`), minus the units that match at least one `except` pattern exactly.

Several include patterns form a union. Several exceptions form a union. There is no
intersection.

### 4.2 `must_not_use`

The forbidden targets are the union of `any_of` and `unverified`. The two lists differ only in
the existence check of section 5.

A reference fact violates the rule when all of these are true:

1. Its unit is selected.
2. None of its tags is in `ignore_tags`.
3. Its kind passes the `kinds` filter.
4. Its certainty is `exact`, or `candidates` is `include`.
5. At least one path in the list `[target, aliases...]` is *forbidden here*. A path is
   forbidden here when a forbidden target matches within it and the owner of the path is not
   the fact's own unit. (A unit may always use itself.)

The violation reports the first path of the list that is forbidden here. If that path is not
equal to `target`, segment by segment, the violation also carries `written`, which is `target`.

One fact gives at most one violation per rule.

### 4.3 `must_only_use`

The allowed targets are the union of `any_of` and `unverified`.

A reference fact violates the rule when checks 1 to 4 of section 4.2 are true and no path in
the list `[target, aliases...]` is *allowed here*. A path is allowed here when an allowed
target matches within it, or when the owner of the path is the fact's own unit.

So a reference is allowed when at least one of its names is allowed: the name in the code, or
the definition behind a re-export.

The violation reports `target` and has no `written`. One fact gives at most one violation.

### 4.4 `must_not_contain`

A marker fact violates the rule when its unit is selected, none of its tags is in
`ignore_tags`, and its marker is in the rule's marker list. The `kinds` and `candidates`
settings do not apply.

### 4.5 `must_be_acyclic`

The rule builds a graph and reports its cycles.

**Nodes.** Without `slices`, each selected unit is a node, and the path of the node is the
path of the unit. With `slices`, the *slice* of a unit is the shortest prefix of its path that
matches at least one slice pattern exactly. Each slice of a selected unit is a node, and the
path of the node is that prefix. A selected unit without a slice belongs to no node. Nodes
are ordered by the first unit of `units` that belongs to them.

**Edges.** A reference fact gives an edge for each path `P` in `[target, aliases...]` when
checks 1 to 4 of section 4.2 are true and the following holds. `P` has an owner. The owner of
`P` is a selected unit that belongs to a node, and that node is not the node of the fact's
unit. The edge goes from the node of the fact's unit to the node of the owner. So a use
through a re-export gives an edge to the unit that re-exports and an edge to the unit of the
definition. Two nodes have at most one edge in each direction. The first fact that gives an
edge is the *witness* of the edge, and the first such path of that fact is the path of the
edge. The edges of a node are ordered by their witnesses, in fact order, and the edges of
one witness by their paths, in the order of the list.

**Cycles.** A *tangle* is a strongly connected component of the graph that has two or more
nodes. Each tangle gives one violation. Tangles are ordered by their first node. The
violation shows one cycle of the tangle, found as follows. Start at the first node of the
tangle. Search breadth-first, and visit the edges of each node in their order. The search
stops at the first edge that leads back to the start. The cycle is the path of the search to
that edge, and that edge. It is a shortest cycle through the start node.

The violation has:

- `unit`: the path of the start node;
- `file`, `line`, `column`: those of the witness of the first step. The violation has no
  `note`: the note of a witness is about one reference, not about the tangle;
- `cycle`: one step for each edge of the cycle, in order. A step has `from`, `to` (node
  paths), `file`, `line`, `column`, `target` (the path `P`) and `kind`, from the witness;
- `size`: the number of nodes in the tangle.

One tangle can hold many cycles. The rule reports one of them. After that cycle is broken,
the next evaluation reports another cycle if the tangle remains.

### 4.6 `must_be_independent`

The rule forbids every reference from one slice to another slice. Slices are those of
section 4.5.

A reference fact violates the rule when checks 1 to 4 of section 4.2 are true, the fact's
unit has a slice `S`, and at least one path in `[target, aliases...]` is *in another slice*.
A path is in another slice when its owner is a selected unit that has a slice, and that slice
is not `S`.

The violation reports the first path of the list that is in another slice, with `written` as
in section 4.2. One fact gives at most one violation.

A unit that is not selected is in no slice: it may use the slices, and the slices may use it.
This is how a shared part is left out.

### 4.7 `must_be_used`

The *subtree* of a unit `U` is the set of units that `U` is a prefix of. `U` is in its own
subtree.

A *use* of `U` is a reference fact for which all of these are true:

1. Checks 2 to 4 of section 4.2 are true.
2. The fact's unit is not in the subtree of `U`.
3. At least one path in `[target, aliases...]` has an owner in the subtree of `U`.

Each selected unit without a use gives one violation. A unit is thus used when code outside
it and outside the units below it refers to it or to a unit below it. The unit of the fact
does not have to be selected.

The violation has `unit`, and no reference. Its `file` is the `file` of the first fact whose
unit is `U`, or empty text when no fact has that unit. Its `line` and `column` are 0, and it
has no `note`.

### 4.8 The unit graph and `must_not_reach`

The *unit graph* has one node for each unit, selected or not. A reference fact gives an edge
for each path `P` in `[target, aliases...]` when checks 2 to 4 of section 4.2 are true and
the following holds. `P` has an owner, and the owner of `P` is not the fact's unit. The edge
goes from the fact's unit to the owner. Witnesses, the paths of the edges and the order of
the edges are as in section 4.5.

`must_not_reach` forbids a use of the targets through any chain of units. The forbidden
targets are those of section 4.2.

A unit *touches* a target when it has a reference fact for which checks 2 to 4 and check 5
of section 4.2 are true. The first such fact of the unit, in fact order, is the *end* of the
unit, and the first path of that fact that is forbidden here is the *end path*.

For each selected unit `S`, search the unit graph breadth-first from `S`, and visit the edges
of each unit in their order. Take the units in the order in which the search finds them, `S`
first. The first of them that touches a target stops the search and gives one violation. A
selected unit whose search finds no such unit gives no violation.

The violation has:

- `unit`: `S`;
- `chain`: one step for each edge on the path of the search from `S` to the unit that touches
  the target, as in section 4.5. It is empty when `S` touches the target itself;
- `end`: `unit` (the unit that touches the target), `file`, `line`, `column` and `kind` of the
  end, and `target` (the end path);
- `file`, `line`, `column`, `note`: those of the witness of the first step, or those of the
  end when `chain` is empty.

### 4.9 `must_be_reachable_from`

The *roots* are the units that match at least one `roots` pattern exactly. A unit is
*reached* when it is a root, or when a path of edges of the unit graph (section 4.8) leads
from a root to it. A unit is *reachable* when a unit of its subtree (section 4.7) is reached.

Each selected unit that is not a root and is not reachable gives one violation. Its fields
are as in section 4.7.

### 4.10 `must_be_in`

Each selected unit that no pattern of `places` matches exactly gives one violation. Its fields
are as in section 4.7. A binding uses this rule to demand that every unit is in one of a set
of places, such as the layers of an architecture.

When a pattern of `places` matches a unit of the fact set whose path is a proper start of
the path of the unit, and no pattern of `places` matches a unit below that one, the
violation has the `note` ``a place matches a unit above it: a pattern that ends in
`**` also matches the units below``. A report prints it as ` (<note>)`.

### 4.11 `must_only_be_used_by`

The selected units are *protected*. The rule forbids every reference to a protected unit from
a unit that is neither protected nor a *user*. A user is a unit that at least one pattern of
`users` matches exactly.

A reference fact violates the rule when checks 2 to 4 of section 4.2 are true, the fact's
unit is not protected and is not a user, and at least one path in `[target, aliases...]` has
an owner that is protected. Check 1 of section 4.2 does not apply: the facts of the rule are
those of the other units.

The violation reports the first path of the list that has a protected owner, with `written`
as in section 4.2. One fact gives at most one violation.

### 4.12 `items`

This rule is about declarations, not about references. A *glob* is one pattern segment
(section 2): it is invalid when it is empty or contains `**`. A glob matches a name when it
matches the name as a path of one segment.

A declaration is *selected* when all of these are true:

1. Its unit is selected by the subjects.
2. None of its tags is in `ignore_tags`.
3. `named` is `none`, or a glob of `named` matches its name.
4. `kinds` is `none`, or its kind is in `kinds`.
5. `visibilities` is `none`, or its visibility is in `visibilities`.
6. For each entry of `related`, it has a relation whose `relation` is that of the entry and
   whose target a pattern of the entry's `any_of` matches within.
7. `modifiers` is `none`, or one of its modifiers is in `modifiers`.

Each selected declaration that does not meet `must` gives one violation:

- `not_exist`: no selected declaration meets it.
- `reside_in`: a pattern matches the unit of the declaration exactly.
- `be_named`: a glob matches its name.
- `have`: it has a relation as in check 6, with the relation and the patterns of `have`.
- `have_modifier`: one of its modifiers is in the list.

Within a list, the patterns form a union. The filters 3 to 7 each narrow the selection, as a
filter does; a pattern that matches nothing is still an error, so a filter cannot select
nothing by mistake.

The violation has the `unit`, `file`, `line` and `column` of the declaration, no `note`,
and the detail `item`: the path of the declaration as `item`, and its kind as `item_kind`.
The settings `kinds` and `candidates` of the rule do not apply.

**Demands about uses.** `not_use` and `only_use` are about the references inside the
selected declarations, not about the declarations. A reference fact is *inside* a selected
declaration when it has an `item` and the path of a selected declaration is a prefix of that
item. So a declaration of a type holds the code of its fields and methods. A fact is checked
when it is inside a selected declaration and checks 2 to 4 of section 4.2 are true. The
subjects select the declarations; the unit of the fact need not be selected.

- `not_use`: the fact gives a violation when a pattern matches within a path of
  `[target, aliases...]`. Unlike section 4.2, the path may be owned by the unit of the fact.
  The violation reports the first such path, with `written` as in section 4.2.
- `only_use`: the fact gives a violation when no path of `[target, aliases...]` is owned by
  the unit of the fact or matched within by a pattern. The violation reports `target`, as in
  section 4.3.

The violation has the `unit`, `file`, `line`, `column` and `note` of the fact, and the detail
`uses` with the `item` of the fact. One fact gives at most one violation. The settings `kinds`
and `candidates` apply, and so do the exemptions.

### 4.13 Exemptions

An exemption lets some references pass that the rule would otherwise see. A reference fact is
*exempt* when, for at least one exemption, a `from` pattern matches the fact's unit exactly
and a `to` pattern matches within at least one path in `[target, aliases...]`.

Wherever this specification applies check 2 of section 4.2, an exempt fact fails it as well:
the rule does not see the fact. This holds for every constraint that reads references, also
for the edges of sections 4.5 and 4.8 and for the uses of section 4.7. Marker facts are never
exempt.

An exemption is *used* when at least one reference fact passes checks 2 to 4 of section 4.2,
not counting the exemptions, and is exempt by this exemption. An exemption that is not used
is an error (section 5), so that an exemption cannot outlive the reference that it lets pass.
`must_not_contain`, `must_be_in`, `must_not_exceed_lines` and `items` without a demand about
uses read no references, so no exemption of such a rule is used.

### 4.14 Order

Violations of `must_not_use`, `must_only_use`, `must_not_contain`, `must_be_independent` and
`must_only_be_used_by` are returned in the order of the facts. Violations of `must_be_acyclic`
are returned in the order of the tangles. Violations of `must_be_used`, `must_not_reach`,
`must_be_reachable_from` and `must_be_in` are returned in the order of `units`. Violations of
`items` are returned in the order of the declarations, and those of a demand about uses in
the order of the facts. Violations of `must_not_exceed_lines` are returned in the order of
`units`. A check of the user (section 4.15) returns its violations in its own
order.

### 4.15 Checks of the user

Every constraint is evaluated in the same two steps, and a binding may give a constraint of
its own, `custom`, that takes the same steps:

1. The check reads its arguments before anything else of the rule is evaluated, after the
   subjects. It reports what is wrong with them as errors of section 5, with the codes and
   roles of this specification: a target that matches nothing is `pattern-matches-nothing`
   with the role `target`, as for `must_not_use`.
2. When the rule has no error, the check finds the violations in what the rule sees: the
   selected units, the references that pass checks 2 to 4 of section 4.2 and are not exempt
   (section 4.13), the markers of the selected units that pass check 2, and the declarations
   of the selected units that pass check 2. It gives a violation any detail of this
   specification, or the detail `custom`: `{ message, key }`, where `key` may be absent.

The check says whether it reads references. When it does not, no exemption of its rule is
used (section 4.13). A check of the user reads references unless it says otherwise.

The rule names the properties of references that its check reads, in `properties`. A name
that is not in the fact set's `properties` is an error (section 5, check 12), so a check
does not pass because it reads a property that no reference can have.

**Checks of items.** A rule about items (section 4.12) takes the same steps, and its demand
may be a check of the binding, `custom`. In step 1 the filters of the rule are checked first,
as for every rule about items, and then the arguments of the check. In step 2 the check also
gets the selected declarations, in their order, and the references that are inside them, in
the order of the facts: the references of step 2 that have an `item` with a prefix that is
the path of a selected declaration.

**Arguments.** A check reports the errors of its arguments as the built-in constraints do
(section 5): targets as `must_not_use` does (checks 7 to 9), unit patterns as `must_be_in`
does, slice patterns as `must_be_acyclic` does, and markers, item kinds, visibilities,
relations, modifiers and the globs of names as section 5.1 has them.

**Graphs.** A check may use the graphs of this specification: the graph of the nodes of
section 4.5, with its tangles and a shortest cycle through the first node of each, and the
unit graph of section 4.8, with the units that are reachable from some units and a shortest
chain from a unit to the nearest unit of a set, found as section 4.8 finds it.

All built-in constraints and demands are such checks. Only `custom` is not plain data, so it
has no case (section 6), and `evaluate` (section 8) does not take it: a binding evaluates it
with `select`.

A `custom` violation is reported as `<place>[<rule>] <unit> <message>`, followed by
` (<note>)` when it has a note.

### 4.16 `must_not_exceed_lines` and `not_exceed_lines`

These rules are about the lines of section 3. A limit is a whole number of 0 or more.

`must_not_exceed_lines`: each selected unit that has more lines than `lines` gives one
violation. A unit with exactly `lines` lines gives none, and a unit that has no lines gives
none. The fields of the violation are as in section 4.7, with one change: when no fact has
the unit, the `file` is that of the first declaration of the unit, or empty when the unit has
no declaration.

`not_exceed_lines` is a demand of `items` (section 4.12): each selected declaration that has
more lines than the limit gives one violation, at the place of the declaration. A
declaration that has no lines gives none.

The detail is `too-long`, with `lines`, the lines of the unit or of the declaration, and
`limit`. For a declaration it also has the path of the declaration as `item` and its kind as
`item_kind`. The settings `kinds` and `candidates` of the rule do not apply, and no exemption of
the rule is used (section 4.13).

The report is `<place>[<rule>] <unit> has <lines> lines, and the limit is <limit>` for a
unit, and `<place>[<rule>] <unit> declares <kind> <item>, which has <lines> lines, and the
limit is <limit>` for a declaration.

### 4.17 Diagrams

A *diagram* is the graph that a rule sees, as text. It is not a check: it has no violations,
and the constraint of the rule is not evaluated.

**Graph.** The graph is that of section 4.5, with the subjects, the filters and the
exemptions of the rule. The slices are those of the constraint when it is `must_be_acyclic`
with `slices` or `must_be_independent`, and there are none otherwise. A *cycle node* is a
node in a tangle, and a *cycle edge* is an edge between two nodes of one tangle. The cycle
nodes and the cycle edges are *marked*.

**Layers.** A diagram can instead have *layers*, a list from the top to the bottom. Each
layer has a name, a list of unit patterns and `must_not_use`, a list of the positions of
other layers in the list, starting at 0. Then the nodes are the layers: node `i` is layer `i`,
and its label is the name of the layer. A subject is in the first layer that has a pattern
that matches it exactly, and a subject in no layer is in no node. A unit that is not a
subject is in no node. The edges are made as for the slices of section 4.5. An edge from a
layer to a layer in its `must_not_use` is marked, and no node is marked. A position that is
not in the list is ignored. A binding draws its layers with these: the layers that each
layer must not use, and subjects `all`.

**Errors.** A diagram has the errors of section 5 that are not about the constraint: those
of the name, the subjects, the filters and the exemptions, and those of the slices. A graph
with fewer than two nodes is no error (check 6 does not apply). The errors are in the order
of section 5. With layers, the errors of the patterns of the layers take the place of the
checks of the constraint, in the order of the layers: role `layer`, `empty-list` for an empty
list, and the errors of check 3 for each pattern.

**Text.** Node `i`, in the order of the nodes, has the name `n<i>` and the path of the node as
its label, printed as section 3 has it. The edges are written in the order of their nodes,
and the edges of a node in their order. `<color>` is `#d62728`. `<direction>` is `TB`, from
the top to the bottom, with layers, and `LR`, from left to right, otherwise. `<name>` is the name of the
rule with `\` written as `\\` and `"` as `\"`. Each line ends with a line break.

`mermaid` is a flowchart:

```
---
title: "<name>"
---
flowchart <direction>
    n<i>["<label>"]                                   one line for each node
    n<from> --> n<to>                                 one line for each edge
    classDef cycle stroke:<color>,stroke-width:2px    only when a marked node exists
    class n<i>,n<j>,... cycle                         the marked nodes, in order
    linkStyle <e>,<f>,... stroke:<color>,stroke-width:2px    only when a marked edge exists
```

The `linkStyle` line lists the marked edges by their number in the order of the edges,
starting at 0. In a label, `#` is written as `#35;`, `"` as `#quot;`, `<` as `#lt;` and `>` as
`#gt;`, in one pass.

`dot` is a Graphviz digraph:

```
digraph {
    label="<name>";
    rankdir=<direction>;
    node [shape=box];
    n<i> [label="<label>"];                   a marked node adds `, color="<color>"`
    n<from> -> n<to>;                         a marked edge adds ` [color="<color>"]`
}
```

In a label, `\` is written as `\\` and `"` as `\"`.

## 5. Errors

A rule that cannot fail is an error, not a pass. Evaluation returns errors and no violations
when at least one of the following checks fails. Each list is checked in the order given, and
the errors are returned in this order.

First, the fact set. If it is invalid, the result is only these errors, one for each distinct
offending value in fact order:

| code | role | value |
|---|---|---|
| `invalid-facts` | `unit`, `kind`, `marker`, `tag`, `property`, `item-kind`, `visibility`, `relation` or `modifier` | the offending value |

Then the rule:

| # | check | code | role | value |
|---|---|---|---|---|
| 1 | `name` is empty | `empty-name` | | |
| 2 | `include` is `any_of []` | `empty-list` | `subject` | |
| 3 | an include pattern is invalid | `invalid-pattern` | `subject` | pattern |
| 3 | an include pattern matches no unit | `pattern-matches-nothing` | `subject` | pattern |
| 4 | `except` is `[]` | `empty-list` | `subject-exception` | |
| 5 | an exception is invalid | `invalid-pattern` | `subject-exception` | pattern |
| 5 | an exception matches no unit | `pattern-matches-nothing` | `subject-exception` | pattern |
| 6 | checks 2 to 5 gave no error, and no unit is selected | `no-subjects` | | |
| 7 | (uses) no pattern in either list | `empty-list` | `target` | |
| 8 | (uses) a pattern of `any_of` is invalid | `invalid-pattern` | `target` | pattern |
| 8 | (uses) a pattern of `any_of` is internal and not grounded | `pattern-matches-nothing` | `target` | pattern |
| 9 | (uses) a pattern of `unverified` is invalid | `invalid-pattern` | `unverified-target` | pattern |
| 7 | (`must_not_contain`) the list is `[]` | `empty-list` | `marker` | |
| 8 | (`must_not_contain`) a marker is not in the fact set's `markers` | `unknown-marker` | `marker` | marker |
| 7 | (`must_be_acyclic`) `slices` is `[]` | `empty-list` | `slice` | |
| 8 | (`must_be_acyclic`) a slice pattern is invalid | `invalid-pattern` | `slice` | pattern |
| 8 | (`must_be_acyclic`) a slice pattern matches no prefix of any unit exactly | `pattern-matches-nothing` | `slice` | pattern |
| 9 | (`must_be_acyclic`) checks 1 to 8 gave no error, and fewer than two nodes exist | `too-few-nodes` | | |
| 7 | (`must_be_reachable_from`) `roots` is `[]` | `empty-list` | `root` | |
| 8 | (`must_be_reachable_from`) a root pattern is invalid | `invalid-pattern` | `root` | pattern |
| 8 | (`must_be_reachable_from`) a root pattern matches no unit | `pattern-matches-nothing` | `root` | pattern |
| 9 | (`must_be_reachable_from`) checks 1 to 8 gave no error, and every selected unit is a root | `no-subjects` | | |
| 7 | (`must_be_in`) `places` is `[]` | `empty-list` | `place` | |
| 8 | (`must_be_in`) a place pattern is invalid | `invalid-pattern` | `place` | pattern |
| 8 | (`must_be_in`) a place pattern matches no unit | `pattern-matches-nothing` | `place` | pattern |
| 7 | (`items`) `named` is `[]` | `empty-list` | `name` | |
| 7 | (`items`) a glob of `named` is invalid | `invalid-pattern` | `name` | glob |
| 7 | (`items`) a glob of `named` matches the name of no declaration | `pattern-matches-nothing` | `name` | glob |
| 8 | (`items`) `kinds` is `[]` | `empty-list` | `item-kind` | |
| 8 | (`items`) a kind is not in the fact set's `item_kinds` | `unknown-item-kind` | `item-kind` | kind |
| 9 | (`items`) `visibilities` is `[]` | `empty-list` | `visibility` | |
| 9 | (`items`) a visibility is not in the fact set's `visibilities` | `unknown-visibility` | `visibility` | visibility |
| 10 | (`items`, for each entry of `related`) the relation is not in the fact set's `relations` | `unknown-relation` | `relation` | relation |
| 10 | (`items`, for each entry of `related`) `any_of` is `[]` | `empty-list` | `related` | |
| 10 | (`items`) a pattern of `related` is invalid | `invalid-pattern` | `related` | pattern |
| 10 | (`items`) a pattern of `related` matches within no target of a relation of that kind | `pattern-matches-nothing` | `related` | pattern |
| 10 | (`items`) `modifiers` is `[]` | `empty-list` | `modifier` | |
| 10 | (`items`) a modifier is not in the fact set's `modifiers` | `unknown-modifier` | `modifier` | modifier |
| 11 | (`items`, `reside_in`) the checks of `places` in `must_be_in` | | `place` | |
| 11 | (`items`, `be_named`) the list is `[]` | `empty-list` | `required-name` | |
| 11 | (`items`, `be_named`) a glob is invalid | `invalid-pattern` | `required-name` | glob |
| 11 | (`items`, `have`) the relation is not in the fact set's `relations` | `unknown-relation` | `relation` | relation |
| 11 | (`items`, `have`) `any_of` is `[]` | `empty-list` | `required` | |
| 11 | (`items`, `have`) a pattern is invalid | `invalid-pattern` | `required` | pattern |
| 11 | (`items`, `have`) a pattern is internal and not grounded (section 5.1) | `pattern-matches-nothing` | `required` | pattern |
| 11 | (`items`, `have_modifier`) the list is `[]` | `empty-list` | `required-modifier` | |
| 11 | (`items`, `have_modifier`) a modifier is not in the fact set's `modifiers` | `unknown-modifier` | `required-modifier` | modifier |
| 11 | (`items`, `not_use` or `only_use`) the checks 7 to 9 of the uses | | `target` or `unverified-target` | |
| 12 | (`items`, `not_use` or `only_use`) checks 1 to 11 gave no error, and no fact of the fact set has an `item` | `no-subjects` | | |
| 12 | (`items`, `not_exceed_lines`) checks 1 to 11 gave no error, and no declaration of the fact set has an `end_line` | `no-subjects` | | |
| 7 | (`must_not_exceed_lines`) checks 1 to 6 gave no error, and no selected unit has lines | `no-subjects` | | |
| 7 | (`must_only_be_used_by`) `users` is `[]` | `empty-list` | `user` | |
| 8 | (`must_only_be_used_by`) a user pattern is invalid | `invalid-pattern` | `user` | pattern |
| 8 | (`must_only_be_used_by`) a user pattern matches no unit | `pattern-matches-nothing` | `user` | pattern |
| 10 | `kinds` is `only []` | `empty-list` | `kind` | |
| 11 | a kind of the filter is not in the fact set's `kinds` | `unknown-kind` | `kind` | kind |
| 12 | a tag of `ignore_tags` is not in the fact set's `tags` | `unknown-tag` | `tag` | tag |
| 12 | a name of `properties` is not in the fact set's `properties` | `unknown-property` | `property` | name |
| 13 | an exemption has `from: []` | `empty-list` | `exemption-from` | |
| 13 | a `from` pattern is invalid | `invalid-pattern` | `exemption-from` | pattern |
| 13 | a `from` pattern matches no unit | `pattern-matches-nothing` | `exemption-from` | pattern |
| 13 | an exemption has `to: []` | `empty-list` | `exemption-to` | |
| 13 | a `to` pattern is invalid | `invalid-pattern` | `exemption-to` | pattern |
| 13 | a `to` pattern is internal and not grounded (section 5.1) | `pattern-matches-nothing` | `exemption-to` | pattern |
| 14 | checks 1 to 13 gave no error, and an exemption is not used | `unused-exemption` | `exemption` | the exemption |

A rule has one constraint, so only the checks of one constraint apply; the numbers 7 to 11
order the checks of one constraint. A glob of `be_named` need not match an existing name: it
says what a name must look like. "(uses)" means
`must_not_use`, `must_only_use` and `must_not_reach`, which have the same checks.
`must_be_independent` has the checks of `must_be_acyclic`; its `slices` are never `none`,
and its nodes are the slices of the selected units. `must_be_used` has no check of its own.

Check 13 runs for each exemption in order, first on its `from` list and then on its `to`
list. Check 14 gives one error for each exemption that is not used, in order. Its value is
the `from` patterns, then ` -> `, then the `to` patterns, each list joined with `, `.

A graph with fewer than two nodes cannot have a cycle, and one slice cannot use another, so
those rules cannot fail. A root is never a violation of `must_be_reachable_from`, so a rule
that selects only roots cannot fail.

Every pattern is checked on its own. A list with one good pattern and one that matches nothing
is an error. This is the property that stops a longer list from silently weakening a rule.

### 5.1 Internal and grounded target patterns

The core can check that a target exists only for targets inside the analysed project.

- A target pattern is *internal* when its first segment has no `*` and is equal to the first
  segment of at least one unit.
- An internal pattern is *grounded* when at least one of these is true:
  - it matches within some path of `units` or `items`;
  - some item path is equal to the first `k` segments of the pattern, these `k` segments have
    no `*`, and the pattern has more than `k` segments. (The pattern names something inside a
    known item. The fact set does not list what items contain.)

Patterns that are not internal (for example `std::fs`) are not checked. Patterns in
`unverified` are not checked. `unverified` exists for internal names that the front end cannot
see, such as items that a macro generates.

## 6. JSON encoding of the cases

Each file in `cases/` is one object:

```
{
  "description": "...",
  "separator": "::",
  "facts": { "units": [...], "unit_lines": [...], "items": [...], "kinds": [...],
             "markers": [...], "tags": [...], "facts": [ fact, ... ] },
  "cases": [ { "name": "...", "rule": rule, "expect": expectation }, ... ]
}
```

- A path or a pattern is a string, which is split at the file's `separator`, or an array of
  segments, which is used as it is. Arrays allow segments that contain the separator, and
  allow invalid patterns to be written.
- In `facts`, the keys `unit_lines`, `items`, `kinds`, `markers`, `tags` default to `[]`.
- In `facts`, `declarations`, `item_kinds`, `visibilities`, `relations`, `modifiers` and
  `properties` default to `[]`. A declaration is `{ "path", "name", "unit", "file", "line", "column",
  "end_line", "tags", "kind", "visibility", "relations", "modifiers" }`; `column` defaults to
  1, `tags`, `relations` and `modifiers` to `[]`, `end_line` may be absent, and a relation is
  `{ "relation", "target" }`.
- A fact is `{ "unit", "file", "line", "column", "tags", "note", "item", "reference" | "marker" }`.
  `item` is a path and may be absent.
  `column` defaults to 1, `tags` to `[]`. `reference` is `{ "target", "aliases", "kind",
  "certainty", "properties" }`; `aliases` defaults to `[]`, `certainty` to `"exact"`, and
  `properties`, an object of texts by their names, to none. `marker` is a string.
- A rule is

  ```
  { "name": "...",
    "reason": "...",                                           (optional)
    "severity": "error" | "warning",                           (optional)
    "subjects": { "all": true } | { "any_of": [pattern, ...] },   plus optional "except": [...]
    "must_not_use": { "any_of": [...], "unverified": [...] }   (each key optional)
    "must_only_use": { "any_of": [...], "unverified": [...] }  (each key optional)
    "must_not_contain": [marker, ...]
    "must_be_acyclic": { "slices": [pattern, ...] }            (the key is optional)
    "must_be_independent": { "slices": [pattern, ...] }
    "must_be_used": {}
    "must_not_reach": { "any_of": [...], "unverified": [...] } (each key optional)
    "must_be_reachable_from": { "roots": [pattern, ...] }
    "must_be_in": { "places": [pattern, ...] }
    "must_only_be_used_by": { "users": [pattern, ...] }
    "must_not_exceed_lines": { "lines": number }
    "items": { "named": [glob, ...], "kinds": [...], "visibilities": [...],
               "related": [{ "relation": "...", "any_of": [pattern, ...] }, ...],
               "modifiers": [...],
               "must": "not_exist" | { "reside_in": [pattern, ...] } | { "be_named": [glob, ...] }
                       | { "have": { "relation": "...", "any_of": [pattern, ...] } }
                       | { "have_modifier": [modifier, ...] }
                       | { "not_use": { "any_of": [...], "unverified": [...] } }
                       | { "only_use": { "any_of": [...], "unverified": [...] } }
                       | { "not_exceed_lines": number } }
                                                               (each filter key optional)
    "kinds": { "only": [...] } | { "except": [...] }
    "ignore_tags": [...],
    "candidates": "include" | "ignore",
    "exemptions": [{ "from": [pattern, ...], "to": [pattern, ...] }, ...],
    "properties": [name, ...] }
  ```

  Exactly one of the constraints is present.
- A case can have `"baseline": "text"`, a baseline file as one string (section 7). Then the
  expectation is checked after the baseline is applied.
- A case can have `"diagram": "mermaid" | "dot"`. Then the case is about the diagram of the
  rule (section 4.17), and the expectation is `{ "diagram": "text" }`, which must be equal to
  the text, or `{ "errors": [...] }`. The case can also have `"layers": [{ "name": "...",
  "units": [pattern, ...], "must_not_use": [position, ...] }, ...]`; an absent
  `must_not_use` is `[]`. Then the nodes are these layers.
- An expectation is `{ "violations": [...] }` or `{ "errors": [...] }`. With a baseline it can
  also have `"stale": [...]`, or be `{ "baseline_error": line }`.
  - The number and order of violations must be equal to the expectation. For each violation,
    every key that the expectation gives must be equal: `unit`, `file`, `line`, `column`,
    `target`, `written`, `kind`, `certainty`, `marker`, `note`, `cycle`, `size`, `chain`,
    `end`, `detail`, `item`, `item_kind`, `lines`, `limit`.
    `"written": null` means that the violation has no `written`, and `"note": null` that it
    has no `note`. `cycle` and `chain` are
    lists of steps; the number of steps must be equal, and in each step every given key must
    be equal: `from`, `to`, `file`, `line`, `column`, `target`, `kind`. `end` is an object;
    every given key must be equal: `unit`, `file`, `line`, `column`, `target`, `kind`.
    `detail` is the form of the violation: `uses`, `contains`, `cycle`, `unused`, `reaches`,
    `unreachable`, `outside`, `item` or `too-long`.
  - `stale` is a list of `{ "unit", "what", "count", "found" }`. It must be equal to the stale
    entries in number, order and all four values. Absent means no stale entry.
  - `baseline_error` is the number of the first line of the baseline that is invalid.
  - Errors must be equal in number, order, `code`, `role` and `value`. An absent `role` or
    `value` means that the error has none. Pattern values are compared as printed with the
    file's separator. This print is for people: two patterns can print the same when a
    segment contains the separator.

## 7. Baseline

A baseline is a list of violations that a project accepts for now. It lets a project start to
use a rule before all violations are gone. The core does not apply a baseline unless the
caller gives one.

### 7.1 Keys

A violation has the key `(rule, unit, what)`:

- `rule` is the name of the rule.
- `unit` is the `unit` of the violation, printed as reports print paths: joined with the
  separator of the fact set, unless the front end has its own print for paths. The same
  holds for the other paths of a key.
- `what` is one of:
  - `uses <kind> <target>` for `must_not_use`, `must_only_use`, `must_be_independent` and
    `must_only_be_used_by`, with the target joined;
  - `contains <marker>`;
  - `unused` for `must_be_used`;
  - `reaches <target>` for `must_not_reach`, with the `target` of `end` joined;
  - `unreachable` for `must_be_reachable_from`;
  - `outside` for `must_be_in`;
  - `item <item>` for `items`, with the path of the declaration joined, and `uses <kind>
    <target>` for its demands about uses;
  - `long` for `must_not_exceed_lines`, and `long item <item>` for `not_exceed_lines`, with
    the path of the declaration joined. The key has no number of lines, so an accepted unit
    or item stays accepted when it grows;
  - `cycle <node> > <node> > ...`, with the `from` of each step joined, in cycle order, turned
    so that the node whose text is smallest comes first;
  - `custom <key>` for a `custom` detail, or `custom <message>` when it has no key. A tab or a
    line break in the text is a space.

For a cycle, the `unit` of the key is that smallest node, not the start node of the search. A cycle
without steps, which only a check of the user can give, has the `unit` of the violation. For a `uses` violation of a demand about uses
(section 4.12), the `unit` of the key is the `item` of the detail, and `what` is
`uses <kind> <target>`.
The key has no line number, so it does not change when lines move.

### 7.2 Entries and text form

An entry is `{ rule, unit, what, count }` with `count` of 1 or more. A baseline file is text.
Each line is one entry: the four values, separated by one tab character. Empty lines and
lines that start with `#` are ignored. A line is invalid when it does not have four values,
when a value is empty, or when `count` is not a whole number of 1 or more in decimal digits.
A rule name that contains a tab or a line break cannot be used with a baseline.

A violation has a *weight*: the `size` of its tangle for a cycle, or 1 when the size is 0,
and 1 for every other violation. So an entry of a cycle accepts a tangle of that many nodes,
and a tangle with more nodes is not accepted, also when its reported cycle stays the same.
The weight is a number of nodes, not their names: a tangle that loses one node and gains
another, with the same reported cycle, is accepted.

When the core writes a baseline, it writes one entry for each key, with the sum of the
weights of the violations that have the key, sorted by `rule`, then `unit`, then `what`, as
text.

### 7.3 Applying a baseline to the violations of one rule

Only the entries whose `rule` is equal to the name of the rule count. The *allowance* of a
key is the sum of `count` of its entries. For each key, let `found` be the sum of the
weights of the violations with that key.

- `found` is not more than the allowance: these violations are accepted and removed.
- `found` is more than the allowance: all violations with that key remain. The core cannot
  tell which of them is new. A cycle that remains gets the note `the baseline accepts a
  tangle of <allowance>`, after its own note and `, ` when it has one. When the allowance
  is 1 and the `size` of the cycle is 2 or more, the note continues with `: a baseline of a version before
  0.2.0 has the number of cycles there, write it again`. A report prints the note of a
  cycle after the size of its tangle: `(the tangle has <size> in all, <note>)`.
- `found` is less than the allowance: the key is *stale*. The stale entry is
  `{ unit, what, count: the allowance, found }`.

The result is the remaining violations in their order, and the stale entries in the order in
which their keys first occur in the baseline. A caller must treat a stale entry as a failure,
so that a baseline can only shrink.

A rule that gives errors (section 5) gives the same errors with a baseline.

## 8. The JSON of the core

A binding in another language than Rust calls the core with JSON requests and gets JSON
answers. The crate `refpact-core` answers them in `wire::Session`, and the crate
`refpact-wasm` gives a session to any language that runs WebAssembly. The shapes are those of
the model above, with two changes: a path or a pattern is a list of segments, never joined
text, and a key of more than one word is written in camelCase, as `itemKinds`, `ignoreTags`
and `anyOf`. A tagged value names its form in `type`, in the words of section 4 joined with
`-`: `{ "type": "must-not-use", "targets": { "anyOf": [...], "unverified": [...] } }`.

A request is an object with `op`. A fact set is sent once and gets a *handle*. Each rule then
names the handle, so that a large fact set crosses the boundary once and not for each rule.

| `op` | Other keys | Answer |
|---|---|---|
| `version` | | `{ version }`, the version of this format: 1 |
| `load-facts` | `facts` | `{ handle }` |
| `drop-facts` | `handle` | `{}` |
| `evaluate` | `handle`, `rule` | `{ ok: true, violations }` or `{ ok: false, errors }` |
| `select` | `handle`, `rule` with a `custom` constraint or a `custom` demand | `{ ok: true, subjects, units, references, markers, declarations, items }` or `{ ok: false, errors }` |
| `graph` | `handle`, `rule` as for `select`, `subjects`, `tangles` | `{ ok: true, nodes, edges }`, `{ ok: true, tangles }` or `{ ok: false, errors }` |
| `diagram` | `handle`, `rule`, `slices`, `layers`, `format` | `{ ok: true, text }` or `{ ok: false, errors }` |
| `print-path` | `path`, `format` | `{ text }` |
| `render-violation` | `violation`, `format` | `{ text }` |
| `render-violations` | `rule` (the name), `reason`, `severity`, `violations`, `format` | `{ text }` |
| `render-error` | `error` | `{ text }` |
| `render-errors` | `rule` (the name), `errors` | `{ text }` |
| `render-stale` | `rule` (the name), `file` (the name of the baseline file), `entries` (stale entries) | `{ text }` |
| `baseline-key` | `violation`, `format` | `{ unit, what }` |
| `baseline-entries` | `violations`, `format` | `{ entries }` |
| `apply-baseline` | `rule` (the name), `violations`, `entries`, `format` | `{ remaining, stale }` |
| `format-baseline` | `entries` | `{ text }` |
| `parse-baseline` | `text` | `{ ok: true, entries }` or `{ ok: false, line }` |

- A `format` is `{ separator, attachedPrefixes, hiddenPrefixes }`: the path format of a fact
  set. The two lists may be absent.
- In a fact set, `unitLines`, `declarations`, `itemKinds`, `visibilities`, `relations`,
  `modifiers`, `properties`, `attachedPrefixes` and `hiddenPrefixes` may be absent, and so may `tags`,
  `aliases` and `properties` (an object of texts by their names) of a fact and `tags`, `relations` and `modifiers` of a declaration. They are then empty. A `note` may be
  absent or `null`, and so may the `endLine` of a declaration.
- In a rule, `kinds` (default `{ "type": "all" }`), `ignoreTags`, `candidates` (default
  `"include"`), `severity` (default `"error"`), `exemptions`, `properties` and the `except` of `subjects`
  may be absent, and so may `unverified` of targets, `named`, `kinds`, `visibilities`,
  `related` and `modifiers` of `items`, and
  `targets` and `units` of a `custom` check.
- An error has `code`, and `role` and `value` when it has them. A violation has `note` when it
  has one, and a `uses` detail has `written` and `item` when it has them. A `too-long`
  detail has `lines` and `limit`, and `item` and `kind` for a declaration. A fact may have
  `item`, and a rule `reason`; the `reason` and the `severity` (default `"error"`) of
  `render-violations` may be absent. The demand `have_modifier` is
  `{ "type": "have-modifier", "modifiers": [...] }`.
- Keys that the core does not know are ignored.

`select` evaluates a rule whose constraint is a check of the binding (section 4.15):
`{ "type": "custom", "targets": [targets...], "units": [[pattern...]...] }`. Both lists may be
absent. Each entry of `targets` is `{ anyOf, unverified }`, and the core checks it as the
targets of `must_not_use`. Each entry of `units` is a list of unit patterns, and the core
checks it as the places of `must_be_in`, with the role `place`. The rule may also be a rule
about items whose `must` is such a check; the core then checks the filters first.

The answer has what the rule sees, by position in the fact set:

- `subjects`: the selected units.
- `units`: for each entry of `units` of the check, the units that one of its patterns matches
  exactly.
- `references`: `{ fact, unit, owners, hits, inside }` for each reference that the rule sees,
  of all units and in the order of the facts. `owners` has the owner of each path of
  `[target, aliases...]`, or `null` for none. `hits` has, for each entry of `targets` of the
  check, the place in that list of the first path that the entry matches within, or `null`;
  it is absent when no entry matches a path. `inside` is `true` when the rule is about items
  and the reference is inside a selected declaration, and absent otherwise.
- `markers`: the marker facts of step 2.
- `declarations`: the declarations of step 2.
- `items`: for a rule about items, the selected declarations. Absent for other rules.

The binding finds the violations, and renders and baselines them with the requests below.

`graph` takes the `handle` and the `rule` of a `select`, and answers a graph of the
references that the rule sees: `{ ok: true, nodes, edges }`, or `{ ok: true, tangles }` when
`tangles` is `true`, so that the tangles are searched only for a check that uses them. The graph is the unit
graph (section 4.8), where the node of a unit is its number. When `subjects` is `true`, it
is the graph of section 4.5 without slices, with one node for each subject. A binding asks
for a graph only when its check uses it.

- `nodes`: for the graph of the subjects, the unit of each node. Absent for the unit graph.
- `edges`: for each node, its edges in their order. An edge is `[to, fact, path]`: the node
  that it leads to, its witness, and the place of the path of the edge in
  `[target, aliases...]`.
- `tangles`: `{ members, cycle }` for each tangle of that graph, found as section 4.5 finds
  them. `members` has the nodes of the tangle in ascending order. `cycle` has the reported
  cycle: each step is `[node, edge]`, with the place of the edge in the edges of the node.

`diagram` gives the diagram of a rule (section 4.17). The constraint of the rule is not
read, so it can be a check of the binding. The binding sends the slices of the diagram as
`slices`, or the layers as `layers`: `[{ name, units: [pattern, ...], mustNotUse: [position,
...] }, ...]`, where an absent `mustNotUse` is `[]`. Both may be absent, and a request with
both is not valid. `format` is `"mermaid"` (the default when it is absent) or `"dot"`. The path format is that of the fact set.

A request that is not valid is not answered. The reason names the value: `facts.facts[3].line:
expected a whole number`, `` unknown request `fly` ``, `no fact set has the handle 2`. Errors in
a rule are not such a reason: they are the answer of `evaluate`, as section 5 has them.

The WebAssembly module exports `memory` and four functions, and imports nothing:

1. `refpact_alloc(len)` reserves `len` bytes and returns their address. The caller writes the
   request there, as UTF-8.
2. `refpact_call(address, len)` answers the request and frees its bytes. It returns 0 for an
   answer and 1 for a request that is not valid.
3. `refpact_answer()` and `refpact_answer_len()` give the address and the length of the text:
   the answer, or the reason. The text stays until the next call.
