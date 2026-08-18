# F# idiom reference

Condensed, cited reference for writing F# in this repository. Prefer a concrete rule over vague advice. This file lives at the `skills/` level, referenced by `writing-fsharp` as `../references/fsharp-idioms.md`.

## Formatting

Microsoft's [F# style guide](https://learn.microsoft.com/dotnet/fsharp/style-guide/formatting): spaces only, Fantomas is canonical if the repo adopts a formatter. `PascalCase` for types/modules/public members, `camelCase` for values/parameters. Prefer `|>` pipelines over nested calls. Minimize `mutable`.

## Component design

[F# component design guidelines](https://learn.microsoft.com/dotnet/fsharp/style-guide/component-design-guidelines): F#-to-F# code (this whole repo, for now) can use idiomatic tuples/curried args/DUs freely — .NET-facing-library conventions (methods/properties, XML docs) only matter once something crosses to a non-F# consumer. Keep helpers `private`/`internal`; expose the minimum public surface.

## Domain modeling

Scott Wlaschin, [*Designing with types: making illegal states unrepresentable*](https://fsharpforfunandprofit.com/posts/designing-with-types-making-illegal-states-unrepresentable/): wrap primitives in single-case DUs to stop mixing up same-shaped values (already done in Summa: `AccountId`). Prefer DUs over bool/enum-plus-nullable-field for explicit state. Smart constructors returning `Result`/`Option` over raw record construction when an invariant must hold (already the pattern here — keep it). Avoid wildcard `_ ->` on closed domain DUs; an added case should force every match site to be revisited.

## Performance

Only apply this section to genuinely hot/large-N paths; a 2-3-entry `Entry list` does not qualify. `[<Struct>]` on small DUs/records and `ValueOption` avoid allocation in hot paths. Tail calls are TCO'd in Release builds only when the recursive call is truly the last operation. `Seq` for public signatures needing laziness/any-collection compatibility, `List` for small pattern-matched immutable data, `Array` for random access/cache-locality/parallelism. Benchmark (BenchmarkDotNet) before trading clarity for speed.

## Error handling

Railway-Oriented Programming ([fsharpforfunandprofit.com/posts/recipe-part2](https://fsharpforfunandprofit.com/posts/recipe-part2/)) composes `Result`-returning functions with `bind`/`map`; reserve `Result` for expected, domain-relevant failures the caller must branch on (validation, business rules — e.g. `LedgerError`), not for infra failures that should throw at a boundary. `Option` means "absent," not "absent for a reason" — use `Result` when the caller needs to know why. Wlaschin's own [*Against Railway-Oriented Programming*](https://fsharpforfunandprofit.com/posts/against-railway-oriented-programming/): don't force the bind/map pipeline where a plain `match` reads better.

## Testing

FsCheck for property-based tests where a real invariant exists to generalize (e.g. "any entry list `Transaction.create` accepts has debits = credits") rather than only hand-picked examples. Watch for tautological properties, missing custom generators for domain types, and time/`Random`-dependent nondeterminism.

## I/O-layer F# (Store, adapters — not the domain core)

Everything above targets `Summa.Ledger.Domain`, which stays pure. `Summa.Ledger.Store` (the Postgres event-store adapter, from Stage 6 onward) is deliberately the opposite: this is where I/O, `Async`/`Task`, and Npgsql belong. Don't apply domain-purity review comments to this project — that boundary is the whole point of the port/adapter split in `docs/architecture.md`.

- **`task { }` over `async { }` for Npgsql-facing code.** [F# task expressions](https://learn.microsoft.com/dotnet/fsharp/language-reference/task-expressions): prefer `task { }` when interoperating extensively with .NET libraries built on `Task` (Npgsql is one) — it avoids the `Async.AwaitTask`/`Async.StartAsTask` conversion overhead at every call and gives a better debugging experience. Reach for `async { }` instead only where its extra compositionality (tailcalls, implicit cancellation-token propagation) is actually needed, per [F# async expressions](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/async-expressions).
- **`use`, not manual disposal.** [F# resource management: the `use` keyword](https://learn.microsoft.com/dotnet/fsharp/language-reference/resource-management-the-use-keyword): a `use` binding calls `Dispose` when the value goes out of scope, including on an exception path. Every `NpgsqlConnection`/`NpgsqlCommand` should be `use`-bound — never left to finalization.
- **Parameters, never string-built SQL.** [Npgsql basic usage](https://www.npgsql.org/doc/basic-usage.html): parameter data is sent to Postgres separately from the SQL text and is never interpreted as SQL, which is what actually prevents injection — not escaping. Any interpolated or concatenated SQL string built from a command argument (an idempotency key, an account id) is a real vulnerability here, not a style nitpick; flag it as a bug, not a cleanup item, in review. Prefer positional over named placeholders where it's free to do so — Npgsql has to parse and rewrite named placeholders since Postgres doesn't support them natively.
- **Error handling still splits the same way, just at a different boundary.** Expected, domain-relevant outcomes the caller must branch on — e.g. the event store port's `Appended seq` vs `Duplicate existingId` from `docs/architecture.md` — stay a `Result`-like return type. Genuine infrastructure failure (connection refused, timeout, malformed connection string) is still fine to let throw at this boundary; catching everything into a `Result` here would be the same "leaky Result" anti-pattern called out above, just relocated.
- **Testing distinction.** Stage 6's acceptance criteria calls for integration tests against a real local container — these are integration tests, not the property-based unit tests FsCheck is for above. Don't conflate the two: an integration test needs the container up and is asserting real Postgres behavior (`ON CONFLICT DO NOTHING ... RETURNING` semantics, the unique constraint actually rejecting a duplicate), not a generalized domain invariant.

## Advanced patterns — evaluate on merit, don't default away from them

Active patterns, computation expressions, and SRTP are legitimate F# tools, and this repo's prime directive ("the owner must understand every line") is a reason to *teach* them deliberately, not a reason to avoid them by default — reflexively picking the plain form denies the owner the chance to learn the idiom that working F# engineers actually reach for. Ask "is this genuinely the idiomatic tool for this shape of problem?" — not "can I get away with not using it?" If the advanced form is the better fit, recommend it explicitly: name the pattern, explain in plain terms what it buys here (what duplication or ceremony it removes) and what it costs (partial active patterns allocate via `option`; SRTP can produce confusing error messages; a computation expression is only worth its own definition if the sequencing pattern it captures actually recurs), and let that explanation double as the lesson. Reserve real skepticism for the case where an advanced form would be reached for just to look sophisticated with no genuine fit to the problem at hand.

## Anti-patterns to flag on review

- Bare primitives standing in for domain concepts instead of single-case DUs.
- `Result`/`Option` used for infra failures, or exceptions used for expected domain-validation failures.
- Wildcard match arms on closed domain DUs.
- `mutable`/`ref` where a fold/`List.map`/recursion would stay pure.
- Nulls/`Unchecked.defaultof` leaking in from interop instead of `Option`.
- Point-free composition chained past 2-3 steps, losing argument meaning.
- Public surface leaking helpers that should be `private`/`internal`.
- Reflection/`obj`/boxing where a generic function or DU would give compile-time safety.
- `.fsproj` compile order not matching actual dependency direction.
- SQL built via string interpolation/concatenation from a command argument instead of Npgsql parameters (`Summa.Ledger.Store` only) — this is a correctness/security bug, not a style nitpick.
- An `NpgsqlConnection`/`NpgsqlCommand` not `use`-bound (`Summa.Ledger.Store` only).
