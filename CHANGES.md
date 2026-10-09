# Changes

## [0.8.0] - 2026-10-09

Behaviour in this release moves closer to Go's `text/template` in several places, so
output can change for templates that relied on the old behaviour. All expectations
below were taken from Go 1.26.6 rendering the equivalent input.

### Added
- Variable reassignment with `=`, so a loop can accumulate into a variable declared
  outside it: `{{ $acc = printf "%s-%d" $acc . }}` (#1, thanks @julienduchesne).
  `$` itself can be reassigned too, as in Go.

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
- A call with arguments on an absent receiver yields `<no value>` rather than
  "cannot be invoked as function", matching Go, which answers for an invalid receiver
  before it considers arguments.

### Removed
- **Breaking:** `ExecError::VarContextToSmall`, which no longer had a construction
  site.

## [0.6.0] - 2021-06-07
### Added
- Maximum template depth
### Changed
- Move from `String` to `thiserror` for errors
- `Context::from` returns `Context` instead of `Result`
