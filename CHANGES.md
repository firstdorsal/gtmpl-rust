# Changes

All three crates in this repository share one version and are released together.

## Unreleased

### Added

- **Go's builtin `slice`**, which was missing from the default function set, so a Go
  template using it failed with "function slice not defined". It takes a string or a
  list with Go's index rules and reports Go's messages for an index out of range, a
  crossed pair, a third index on a string and a type that cannot be sliced. Where Go
  indexes a string by byte and returns invalid UTF-8, an index inside a character is
  refused instead -- a Rust `String` cannot hold the alternative. With
  `helm-functions` enabled, Helm's `slice` replaces it, as Sprig's replaces the
  builtin in Helm itself; a test now pins that precedence.
- **`{{ break }}` and `{{ continue }}`**, as Go has had since 1.18. Scoped the way Go
  scopes them, which is lexically: one that no `{{ range }}` encloses is a parse error,
  as is one in a `{{ define }}` or `{{ block }}` body called from a range, or in the
  range's own `{{ else }}`. An `{{ if }}` or `{{ with }}` in between is not a barrier,
  and a `{{ break }}` in a nested range ends only that one. All twenty-two cases that
  pin this went through Go 1.26.8 first and are recorded in `go_conformance.txt`.
- **`{{ range }}` over an integer**, as Go has had since 1.22.
  `{{ range 3 }}{{ . }}{{ end }}` is `012` and `{{ range $i := 3 }}` binds each index;
  a count of zero or less takes the `{{ else }}` branch, the two-variable form is
  refused the way Go refuses it, and a count above `i64` reports Go's "overflows int"
  rather than trying to iterate. Checked case by case against Go 1.26.8 and recorded
  in `go_conformance.txt`. A fractional float is still refused; a whole one iterates,
  because this engine stores it as an integer.

### Fixed

- **`range` over something it cannot iterate said so in Rust's `Debug`.** The message
  read `invalid range Number(Number { n: U(3) })`, handing the reader this crate's
  internals; it is Go's `range can't iterate over 3` now.
- **`mowsDigest` could never succeed.** All four of its methods — `MD5`, `SHA1`,
  `SHA256`, `SHA512` — read the raw digest bytes as UTF-8, which they are not for any
  practical input, so every call returned "Invalid hash". It returns hexadecimal now,
  as `sha256sum` and every other hash function here does, and an unknown method names
  the four that work. The function had no test over its success path, which is how it
  shipped; it has one per method now, against the values `md5sum`, `sha1sum`,
  `sha256sum` and `sha512sum` give for the same input.

### Changed

- **A context field no longer has to be `Clone`.** `From<Vec<T>>`,
  `From<HashMap<String, T>>` and `From<Option<T>>` take their argument by value and
  then cloned every element out of it, so a struct holding a list of another derived
  struct had to derive `Clone` through the whole tree — and a type that cannot be
  `Clone` could not appear in a context at all. They consume what they are given now.
  Relaxing a bound, so nothing that compiled stops compiling.
- The repository's front page documents how to use the engine rather than how the
  repository is laid out, with a complete example and a function-registration example
  that CI compiles and runs with warnings denied.

## [0.9.0] - 2026-10-12

The value type and the derive macro now live in this repository as `gtmpl_value_ng`
and `gtmpl_derive_ng`, so `Value` is finally ours to fix. Most of what follows is the
consequence: formatting, which was constrained by the old crate, now matches Go.

Expectations were taken from Go 1.26.8 rendering the equivalent input. 237 templates
were diffed against it; 217 match byte for byte, and `gtmpl-ng/tests/go_conformance.txt`
records every one of them. Its header accounts for all 20 that do not: 8 are the same
failure reported in different words, 1 is identical output that Rust's debug quoting
shows differently, and 11 are deliberate. `go_conformance.go` beside it regenerates
the Go side, so none of this has to be taken on trust.

### Breaking

- **`gtmpl_value` and `gtmpl_derive` are replaced by `gtmpl_value_ng` and
  `gtmpl_derive_ng`.** `gtmpl_ng::Value`, `Func`, `FuncError` and `from_value` are
  re-exported unchanged, so code that only touches those keeps compiling. Code that
  uses `#[derive(Gtmpl)]` has to change one import per crate: `Into` is not
  transitive, so a value from the published derive will not satisfy a bound asking for
  the new type, and the error lands at the call site. Alternatively enable
  `gtmpl_value_ng`'s `gtmpl_value_compat` feature and convert with `Value::try_from`.
- **`printf` no longer fails on a bad verb.** Go writes `%!d(string=x)` into the
  output and carries on; so does this. `PrintError` is removed — nothing could
  construct it any more.
- **`ExecError::NullInChain` is removed.** It was unreachable: `ChainNode` has one
  producer, which rejects a `nil` base before building one.
- **`NodeError::NaN` is now `NodeError::IllegalNumberSyntax(String)`**, carrying the
  literal, as Go's message does.
- A float outside the integers is no longer collapsed to one, so `as_u64()` on a
  `Value::from(1e20)` now answers `None` where it used to answer `u64::MAX`.
- **`Display` for `Value` and `Number` writes different text** for containers, nil and
  floats — see the four entries under Changed. This reaches code that only calls
  `value.to_string()` or `format!("{}", value)` and never renders a template, so a
  snapshot test or a log parser downstream has to be revisited.
- **`%+v` no longer signs a number.** Go consumes the `+` for its field-name mode, so
  `{{ printf "%+v" 42 }}` is `42` rather than `+42`. A width or precision beyond a
  million is refused the way Go refuses it, rather than allocating for it.
- The edition moves to 2021 across the workspace, and `rust-version` is declared as
  1.85 — set by `std::ptr::fn_addr_eq`, not by the edition, which needs only 1.56. CI
  checks it on that toolchain. The `helm-functions` feature is above that floor,
  because `uuid` and `time` set their own and cargo cannot declare one per feature.

### Added

- `gtmpl_value_ng::format_float` and `write_float`, which implement Go's
  `strconv.FormatFloat` rules, and `FloatFormat` to pick between them.
- `gtmpl_ng` re-exports the whole value surface — `Value`, `Number`, `NumberKind`,
  `Function`, `Func`, `FuncError`, `FromValue`, `from_value`, `format_float`,
  `write_float`, `FloatFormat` — so a consumer naming any of them does not need a
  second dependency the documented manifest never mentioned.
- `%T`, which names a value's type as Go's does.
- `Number::kind` and `NumberKind`, so a number is read once instead of by trying
  `as_u64`/`as_i64`/`as_f64` in the right order.
- `Number::float`, `Number::signed` and `Number::unsigned`, which build a number
  without the whole-float collapse. The absence of a lossless float constructor was
  what made #13 unfixable from outside.
- `gtmpl_value_ng`'s `gtmpl_value_compat` feature: `TryFrom<gtmpl_value::Value>`, for
  migrating a tree one call site at a time. It is `TryFrom` because a
  `gtmpl_value::Func` names the other crate's `Value` in its own signature and cannot
  be converted at all.
- Hexadecimal float literals: `{{ 0x1.8p1 }}` is `3` (#15).
- Go's `%!…` notation in full: `%!d(string=x)`, `%!d(MISSING)`, `%!(EXTRA int=2)`,
  `%!(NOVERB)`, `%!d(BADINDEX)`, `%!(BADWIDTH)` and `%!(BADPREC)`.
- `#[derive(Gtmpl)]` on a tuple struct, an enum or a union is a compile error with a
  span, reported through `syn::Error` rather than a proc-macro panic. A tuple struct
  used to compile and produce an empty object, dropping every field silently.

### Changed

- **A container renders as Go renders it.** An array used to go through Rust's `Debug`
  — `[String("a"), Bool(true)]` — where Go writes `[a true]`, and a map
  `{"k": String("v")}` where Go writes `map[k:v]`.
- **A map's keys are sorted when formatted**, as Go has sorted them since 1.12.
  0.8.0 fixed the order for `range`; `printf "%v"` on a map was still different from
  run to run.
- **`%v` on a float uses Go's rules**, so `1e20` renders `1e+20` rather than
  `100000000000000000000`, and `+Inf`/`NaN` rather than `inf`/`NaN`.
- `%e` and `%E` write Go's exponent: `1.234568e+03`, not Rust's `e3` (#14).
- `Value::Nil` renders as `<nil>`, the way Go prints a nil, rather than `nil`.
- `serde_yaml` is deprecated and archived upstream; the Helm YAML functions use the
  maintained `serde_yaml_neo` fork. One behaviour follows from it, and it is a fix:
  `fromYaml` now reads a leading-zero integer as octal, so `0644` is `420`, which is
  what Go's YAML readers and Helm do.
- A list's span runs through its `{{end}}`. A branch's own length is only its
  pipeline's span — deliberately, so an error in `{{if .x}}` points at `.x` — and a
  list measured its extent from that, claiming ten bytes of a twenty-byte
  `{{if .cond}}x{{end}}` (#17).

### Fixed

- **A template could no longer take the process down.** A width or precision was read
  with no upper bound and used as an allocation size, so
  `{{ printf "%.18446744073709551615d" 1 }}` panicked on a capacity overflow and a `*`
  width from a large argument aborted outright — past the `Result` the API returns.
  Go caps both at a million; so does this, including its habit of checking the running
  value before taking each further digit, so `%1000001d` is still a width.
- **A hexadecimal float with leading zeros evaluated to the wrong number.** The digits
  past the twentieth collapse into one sticky digit, and the leading zeros pushed the
  significant ones out of that window, so `0x0000…001p0` came out as 2^48 rather than
  1. Silent, and wrong by any factor.
- **`Number`'s comparison was not total.** `Number::signed` can hold a non-negative
  value and `Number::float` a whole one, which the shortcut arms were written without —
  so `signed(5) < unsigned(3)` was true, and a float-valued context answered `true` to
  `ge .x 3` and `false` to `eq .x 3`. The ordering is exact across all three
  representations now, without widening either side to `f64`, and equality is defined
  through it so the two cannot disagree.
- **`serde` could not serialize a struct's fields.** `Value::Object`, which
  `#[derive(Gtmpl)]` produces, was `serde(skip)` and failed at runtime; `Value::Nil`
  did not round-trip. Both work, and the two losses a document genuinely cannot carry
  are documented and tested.
- A precision truncated booleans, nils, characters and whole containers, where Go
  truncates strings only; container elements were formatted with default flags where
  Go passes them down; `%q` and `%x` applied the precision to their own output rather
  than to the input; `%.0d` of zero printed a zero; `%#o` of zero doubled it; `%#O`
  lost its leading zero; `% x` on a string did not separate the bytes; `%U` ignored
  its precision and honoured the zero flag; a bracket naming a missing argument was
  not counted as consumed; a hexadecimal exponent too small for any float errored
  where Go underflows to zero; and `println` wrote `<no value>` where `print` writes
  `<nil>`.
- **`toYaml` no longer fails on an integer.** It read numbers through `as_f64`, which
  answers only for the float variant, so `{{ toYaml .ports }}` over a list of port
  numbers -- most of what a chart passes it -- errored with "Cannot convert number to
  YAML". Output now matches `gopkg.in/yaml.v3`, which is what Helm marshals with.
- **Negative non-integer numbers are no longer truncated** (#13). `{{ -3.14 }}`
  renders `-3.14`. The fix existed on `gtmpl_value`'s master branch, unreleased, since
  February 2025; vendoring the crate is what delivered it.
- **A float outside the integers is no longer replaced by a different number.** The
  whole-float collapse cast with `as`, which saturates, so `Value::from(1e20)` became
  `u64::MAX` and `-1e20` became `i64::MIN`.
- **A float below about 2e-16 is no longer collapsed to zero.** "Whole" was tested as
  `fract() < EPSILON`, which every such magnitude satisfies, so `1e-20` became `0`.
- `-0.0` renders as `-0`.
- `%-5d` left-aligns and `%6.3s` pads on the left. Flag handling was a match over
  thirteen hand-written combinations, and anything it did not list fell through to
  Rust's defaults, which differ from Go's.
- `%#o` writes Go's bare leading zero rather than Rust's `0o` prefix, and `%#X` writes
  `0X`.
- `%q` takes its ASCII-escaping cue from `+`, as Go does, and `#` asks for a
  backquoted string; `%q` on a string is quoted at all, which it was not.
- `%.3d`, `%08.3d` and `%#08x` pad the way Go pads: zeros between the sign and the
  digits, an explicit precision overriding the zero flag, and the `0x` prefix outside
  the zero count.
- `cargo test` no longer reports success having run neither panic suite. Three of
  their cases never needed the optional registry and now run by default; the rest
  report an ignored test naming the feature they need (#16).
- Deprecated `std::f64::EPSILON`-style constants and an unlinted function pointer
  comparison are gone from the vendored crates, which now pass the same
  `clippy -D warnings` gate as the engine.

## [0.8.0] - 2026-10-09

Behaviour in this release moves closer to Go's `text/template` in several places, so
output can change for templates that relied on the old behaviour. All expectations
below were taken from Go 1.26.6 rendering the equivalent input.

### Added
- Variable reassignment with `=`, so a loop can accumulate into a variable declared
  outside it: `{{ $acc = printf "%s-%d" $acc . }}` (#1, thanks @julienduchesne).
  `$` itself can be reassigned too, as in Go.

- Go's full integer literal syntax: hexadecimal, binary and octal with both an `0o`
  prefix and a bare leading zero, and `_` digit separators (#5).
- An action may span lines, as in Go. Only running out of input is an unclosed
  action.

### Changed
- **Map keys are iterated in sorted order**, as Go does. Rendering a template over a
  map is now deterministic; previously the order came from a `HashMap` and differed
  between runs (#1).
- **A field chain through a missing map key yields `<no value>`** instead of an error,
  however deep the chain, and also when the missed value is carried through a
  variable. A receiver of the wrong type, or a nil receiver, still errors (#3).
- **`ErrorContext.len` spans the whole construct** it points at. It previously held the
  length of the construct's first token only, so `{{.a.b}}` reported `2` instead of
  `4`. Code that underlines errors using `col` and `len` will underline a different,
  correct amount (#4).
- Field lookup walks through borrowed values and clones only the value it resolves, so
  its cost no longer depends on the size of the maps it traverses. Measured locally
  with the `render_fields` benchmark over a 1000-field map: `.metadata.name` went from
  ~108µs to ~0.25µs (#2, thanks @julienduchesne) and `$.metadata.name`, which that
  change did not cover, from ~214µs to ~0.27µs in a follow-up. Both are now flat
  across 10, 100 and 1000 fields. Declaring a variable from a large value
  (`{{ $top := . }}`) still copies it — that is the declaration, not the chain.
- `ExecError` and `ParseError` are now `#[non_exhaustive]`, so future error variants
  are no longer breaking changes.

### Fixed
- The `{{ else }}` branch of a `range` no longer runs for a non-empty collection (#1).
- A `range` declaration such as `{{ range $x := . }}` no longer overwrites an outer
  variable of the same name, and `if`/`with`/`range` restore the variable scope they
  entered with (#1).
- Ranging over nil takes the `{{ else }}` branch instead of erroring (#1).
- Two or more spaces before `-}}` no longer break the template. The space belongs to
  the delimiter, and a run of whitespace used to swallow it, after which the `-` was
  lexed as a number and became a stray argument (#7).
- A multi-byte character in an action no longer crashes the lexer thread. `backup()`
  gave back one byte where `next()` had consumed the character's full UTF-8 width,
  leaving the position inside its encoding; the panic surfaced as the unrelated
  "unexpected receiving on a closed channel".
- Deeply nested `{{if}}`/`{{range}}`/`{{with}}` no longer abort the process. The
  parser is recursive descent and had no depth limit, so a few hundred levels in under
  10 KiB of source overflowed the stack -- an abort, not an error, and therefore not
  something a caller could catch. Nesting is now capped at 64 levels with a
  `ParseError`.
- Text ending in a multi-byte character immediately before a `{{-` no longer panics
  the lexer thread. The trailing whitespace a left trim marker removes was measured as
  if every character were one byte, so the trim landed inside the character.
- Whitespace around a trim marker is the four ASCII space characters Go uses, so other
  whitespace -- a non-breaking space, say -- is text and is no longer trimmed away.
- `printf "%v"` works for every value kind. It used to error for a signed integer and
  for a float, so `{{ printf "%v" -5 }}` failed.
- `printf "%f"`, `%e` and `%E` default to six digits after the point, as Go does, and
  accept an integer. `gtmpl_value` stores a whole float as an integer, so refusing the
  verb there would have made `%f` format `3.5` and refuse `3.0`.
- An absent or nil value formats as `<nil>` under `%v` and `print`, instead of erroring
  or printing `nil`.
- A call with arguments on an absent receiver yields `<no value>` rather than
  "cannot be invoked as function", matching Go, which answers for an invalid receiver
  before it considers arguments.
- `0x10` and `017` evaluated to `0` and `17` instead of 16 and 15, and a bare `-` or
  `+` evaluated to `0`. Two steps that promote a float reading to an integer ran even
  when the float parse had failed, so the check that should have rejected a non-number
  never fired (#5).
- A trim marker is recognised after any whitespace Go counts as such, not only a
  literal space, so `{{-<TAB>.a<TAB>-}}` no longer breaks (#6).
- A token that spans a newline -- a raw string, a comment, a multi-line action -- no
  longer panics the lexer thread, which surfaced as the unrelated "unexpected
  receiving on a closed channel". Line and column are derived from the byte offset
  rather than maintained incrementally, which also fixes the line reported after a
  multi-line comment.

### Removed
- **Breaking:** `ExecError::VarContextToSmall`, which no longer had a construction
  site.
- **Breaking:** the payloads of the eight error variants carrying a node are boxed,
  which shrinks `ParseError` from 296 to 72 bytes and `ExecError` from 304 to 88. The
  remaining size is other large variants, not these.

### Removed
- The write-only `is_i64`, `is_u64`, `is_f64` and `number_typ` fields on `NumberNode`,
  and the `NumberType` enum. Nothing ever read them.

### Known issues
- Negative non-integer numbers are truncated: `{{ -3.14 }}` renders `-3`, and so does
  a `-3.14` coming from the context. The cause is in the `gtmpl_value` dependency,
  which misclassifies every negative float as whole, and it cannot be worked around
  from here -- the value is already truncated before this crate sees it.

  The fix exists: it is commit `eff95ed9` on that crate's `master`, written by its own
  author in February 2025, and simply never released -- the latest version on crates.io
  is still 0.5.1 from 2021. A workspace can pick it up today with
  `[patch.crates-io] gtmpl_value = { git = "https://github.com/fiji-flo/gtmpl_value" }`,
  which this crate's full suite passes against. See #13.

## [0.6.0] - 2021-06-07
### Added
- Maximum template depth
### Changed
- Move from `String` to `thiserror` for errors
- `Context::from` returns `Context` instead of `Result`
