# Changes

## [0.8.0] - 2026-10-09
### Added
- Variable reassignment with `=`, so a loop can accumulate into a variable
  declared outside it (`{{ $acc = printf "%s-%d" $acc . }}`) (#1, thanks
  @julienduchesne)
### Changed
- Map keys are iterated in sorted order, as Go does, making output
  deterministic (#1)
- Field lookup walks through borrowed values and clones only the value it
  resolves, so the cost no longer depends on the size of the maps traversed.
  `.metadata.name` over a 1000-field map goes from 108us to 0.26us (#2, thanks
  @julienduchesne), and `$.metadata.name` from 214us to 0.27us
- A field chain through a missing map key yields `<no value>` instead of an
  error, matching Go. A receiver of the wrong type still errors (#3)
### Fixed
- The `{{ else }}` branch of a `range` no longer runs for a non-empty
  collection (#1)
- A `range` declaration such as `{{ range $x := . }}` no longer overwrites an
  outer variable of the same name, and `if`/`with`/`range` restore the variable
  scope they entered with (#1)
- Ranging over nil takes the `{{ else }}` branch instead of erroring (#1)
- Two or more spaces before `-}}` no longer break the template (#7)
- `ErrorContext`'s `len` spans the construct it points at, instead of only the
  construct's first token (#4)
### Removed
- **Breaking:** `ExecError::VarContextToSmall`, which no longer had a
  construction site

## [0.6.0] - 2021-06-07
### Added
- Maximum template depth
### Changed
- Move from `String` to `thiserror` for errors
- `Context::from` returns `Context` instead of `Result`
