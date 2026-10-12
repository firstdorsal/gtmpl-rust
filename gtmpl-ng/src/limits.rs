//! What a single render is allowed to consume.
//!
//! A template is often not written by the program that renders it: a chart, a
//! notification body or a dashboard panel can arrive from a user, and then the
//! template itself is untrusted input. That makes it an attack surface, and the
//! interesting attacks here are not about reading files -- the engine has no I/O at
//! all -- but about making the host spend what it does not have.
//!
//! Four one-line templates took the process down before these limits existed, each
//! past the `Result` the API promises, so no caller could have caught them:
//!
//! | Template | What happened |
//! |---|---|
//! | `{{define "r"}}{{template "r" .}}{{end}}{{template "r" .}}` | stack overflow, `SIGABRT` |
//! | `{{range 1000000000}}{{end}}` | ran until killed |
//! | `{{range 100000}}{{repeat 1000000 "x"}}{{end}}` | allocator failed, `SIGABRT` |
//! | `{{range 10000}}{{range 10000}}{{range 10000}}{{end}}{{end}}{{end}}` | ran until killed |
//!
//! The defaults are sized for templates a human wrote: [`Limits::default()`] leaves
//! every chart in this repository's own test corpus untouched. A program that
//! genuinely needs more raises the one limit it needs, and one that only ever renders
//! templates it wrote itself can use [`Limits::unlimited`].
//!
//! The counters bound memory and stack. Time is bounded only by the opt-in
//! [`Limits::deadline`], which that field's documentation makes the case for.
//!
//! Growth policy: a new limit arrives in a minor release with a default of `None`, so
//! a render that was legal stays legal; an existing default is never lowered except
//! as a documented security fix; and [`Limits::unlimited`] always means every limit,
//! future ones included, is off -- except the fixed nesting and value-depth caps,
//! which exist because no budget makes an exhausted stack safe.
//!
//! ```rust
//! use gtmpl_ng::{Context, Limits, Template, Value};
//!
//! // The default refuses a loop that no hand-written template would ask for.
//! let mut tmpl = Template::default();
//! tmpl.parse("{{ range 100000 }}x{{ end }}").unwrap();
//! assert!(tmpl.render(&Context::empty()).is_err());
//!
//! // Raising just that one limit lets it through.
//! let mut tmpl = Template::with_limits(Limits::default().with_iterations(200_000));
//! tmpl.parse("{{ range 100000 }}x{{ end }}").unwrap();
//! assert_eq!(tmpl.render(&Context::empty()).unwrap().len(), 100_000);
//! ```

use std::cell::Cell;
use std::time::{Duration, Instant};

/// How much work one render may do.
///
/// Set on a [`Template`](crate::Template) through
/// [`with_limits`](crate::Template::with_limits) or the public `limits` field. See the
/// [module documentation](self) for why these exist.
///
/// `None` means unlimited. Each field is independent, so raising one does not weaken
/// the others.
///
/// The struct is `Copy` and `Eq` deliberately, and stays so: the limits in force are
/// read on hot paths out of a thread-local `Cell`, which `Copy` is what makes cheap.
/// A future limit must itself be a small copyable value; one that is not belongs on
/// its own type.
///
/// The struct is `#[non_exhaustive]`: a limit for a resource not yet counted must be
/// able to arrive in a minor release, and a new field would otherwise break every
/// exhaustive construction downstream. Start from [`Limits::default()`] or
/// [`Limits::unlimited()`] and change fields through the `with_*` methods -- or by
/// assignment, since the fields stay public and writable:
///
/// ```rust
/// use gtmpl_ng::Limits;
/// use std::time::Duration;
///
/// let limits = Limits::default().with_deadline(Duration::from_secs(5));
/// assert_eq!(limits.iterations, Some(10_000));
///
/// let mut spelled_out = Limits::default();
/// spelled_out.deadline = Some(Duration::from_secs(5));
/// assert_eq!(spelled_out, limits);
/// ```
///
/// # What is not bounded
///
/// **Time, unless you opt in.** The counters bound memory, stack and iteration count,
/// because those are what end a process: an exhausted allocator aborts, and so does an
/// overflowed stack. How long the allowed work takes is bounded only when
/// [`deadline`](Self::deadline) is set -- that field says why it is not a default --
/// and a deadline stops a render *between* steps, never inside one: a single call into
/// a slow function runs to its end. A caller who must take control back mid-call
/// renders on a thread it can abandon, which works because `Template`, `Context` and
/// `Value` are all `Send`.
///
/// **Parse recursion**, which is held at a fixed 64 nested actions. It is not a knob
/// because there is no safe value above it: nesting is what consumes the real stack
/// during parsing, and a template rejected at 65 is not one anybody wrote by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// Iterations a single `{{range}}` over an integer may run, and elements a single
    /// list-generating function (`until`, `untilStep`, `seq`) may produce.
    ///
    /// This counts only what the *template* asks for. Ranging over an array or map
    /// from the context is not capped here -- that length is the host program's own
    /// data, and a template cannot inflate it -- but every iteration of it still
    /// counts against [`total_iterations`](Self::total_iterations).
    ///
    /// Default: 10 000.
    pub iterations: Option<usize>,

    /// Iterations the whole render may run, summed over every loop and every nested
    /// template.
    ///
    /// This is what stops nesting from multiplying: three loops of 10 000, each
    /// inside the last, are inside [`iterations`](Self::iterations) individually and
    /// are 10¹² together.
    ///
    /// Default: 1 000 000.
    pub total_iterations: Option<usize>,

    /// Bytes the render may write.
    ///
    /// Counted as they are written, so a render that would grow past the limit stops
    /// at it instead of allocating first and failing after.
    ///
    /// Default: 64 MiB.
    pub output_bytes: Option<usize>,

    /// How large a value a single function call may hand back: characters from the
    /// string-generating functions (`repeat`, `indent`, `randAlpha` and the other
    /// `rand*`), and in general the weight -- string bytes plus container elements
    /// and keys -- of any function result.
    ///
    /// The general bound is what stops a value from growing by feeding a function its
    /// own output: `{{ $s = print $s $s }}` doubles per loop iteration and reaches
    /// 2^43 bytes inside forty of them, with every other counter content. The
    /// multiplying functions (`repeat`, `indent`, `replace`) also judge their product
    /// against this *before* allocating, so the refusal comes before the allocation
    /// it refuses.
    ///
    /// Default: 1 000 000, which is also the width `printf` refuses past -- Go's own
    /// `fmt` draws its line in the same place.
    pub function_output: Option<usize>,

    /// How long one render may take.
    ///
    /// **Default: `None`**, and deliberately so. Every other limit here counts
    /// something the template asked for, so the same template and the same data give
    /// the same answer on any machine. A deadline does not: it would make a render
    /// fail on a loaded or slower host and succeed on an idle one, and a templating
    /// engine whose output depends on how busy the box was is worse than one that can
    /// be made slow. So it is opt-in, and it is the right thing to opt into when the
    /// templates are not yours.
    ///
    /// It is checked between units of work -- at least before each function call, and
    /// periodically inside loops; the exact interval is not part of the API -- so it
    /// bounds how long a render keeps going, not how long a single step takes. One
    /// `{{ genCA }}` or one `{{ htpasswd }}` runs to its end whatever the deadline
    /// says; what the deadline stops is the next one. A render is not currently
    /// interrupted mid-call, because a template function is an ordinary call with no
    /// cancellation point in it.
    ///
    /// Neither the stride nor the opt-in costs anything measurable. Over a million
    /// iterations: 26.9 ns each with no deadline, 27.0 ns with one, against 40.4 ns
    /// when the clock was read every iteration -- and the renders above stopped
    /// between 2 and 290 µs past their deadline, the larger figure being the output
    /// buffer growing rather than the stride.
    ///
    /// This is what covers the gap the counters cannot. Two measured renders, both
    /// inside every default above:
    ///
    /// - `{{ range 10000 }}{{ htpasswd "u" "p" }}{{ end }}` is 10 000 bcrypt hashes
    ///   at ~165 ms each: a **27-minute render** out of a flat one-liner, and it
    ///   *succeeds* -- 20 000 bytes of output, every counter far from its limit,
    ///   because bcrypt's cost is time by design and no counter here counts time.
    /// - `{{ range 100 }}{{ range 10000 }}{{ len (repeat 1000000 "x") }}{{ end }}{{ end }}`
    ///   spends the whole million-iteration budget building and discarding a megabyte
    ///   per body: 13.8 seconds before `total_iterations` stops it. A two-second
    ///   deadline stops the same template at 2.00 s.
    ///
    /// ```rust
    /// use gtmpl_ng::{Context, Limits, Template};
    /// use std::time::Duration;
    ///
    /// let mut tmpl = Template::with_limits(
    ///     Limits::unlimited().with_deadline(Duration::from_millis(50)),
    /// );
    /// tmpl.parse("{{ range 100000000 }}{{ end }}").unwrap();
    /// let error = tmpl.render(&Context::empty()).unwrap_err();
    /// assert!(error.to_string().contains("did not finish within"));
    /// ```
    pub deadline: Option<Duration>,
}

impl Limits {
    /// The defaults, as a constant, so a `static` can hold them.
    pub const DEFAULT: Limits = Limits {
        iterations: Some(10_000),
        total_iterations: Some(1_000_000),
        output_bytes: Some(64 * 1024 * 1024),
        function_output: Some(1_000_000),
        deadline: None,
    };

    /// No budgets at all.
    ///
    /// Appropriate when every template rendered comes from the program itself. Do not
    /// use it on a template that came from somewhere else: with this, three of the
    /// four templates in the [module documentation](self) end the process again --
    /// aborting rather than returning, so there is nothing to catch. (The recursive
    /// one stays refused: nesting depth and value depth are fixed caps, not budgets,
    /// because no stack survives lifting them.)
    pub const fn unlimited() -> Limits {
        Limits {
            iterations: None,
            total_iterations: None,
            output_bytes: None,
            function_output: None,
            deadline: None,
        }
    }

    /// These limits with [`iterations`](Self::iterations) replaced.
    ///
    /// Every `with_*` method takes the value or `None`, so `with_iterations(50_000)`
    /// tightens or raises the limit and `with_iterations(None)` lifts it. Zero is a
    /// cap like any other -- it refuses the first iteration -- not a spelling of
    /// unlimited; `None` is that. (A literal infers as `usize`; a variable of another
    /// integer type needs a cast.)
    #[must_use]
    pub fn with_iterations(mut self, limit: impl Into<Option<usize>>) -> Limits {
        self.iterations = limit.into();
        self
    }

    /// These limits with [`total_iterations`](Self::total_iterations) replaced.
    #[must_use]
    pub fn with_total_iterations(mut self, limit: impl Into<Option<usize>>) -> Limits {
        self.total_iterations = limit.into();
        self
    }

    /// These limits with [`output_bytes`](Self::output_bytes) replaced.
    #[must_use]
    pub fn with_output_bytes(mut self, limit: impl Into<Option<usize>>) -> Limits {
        self.output_bytes = limit.into();
        self
    }

    /// These limits with [`function_output`](Self::function_output) replaced.
    #[must_use]
    pub fn with_function_output(mut self, limit: impl Into<Option<usize>>) -> Limits {
        self.function_output = limit.into();
        self
    }

    /// These limits with [`deadline`](Self::deadline) replaced.
    #[must_use]
    pub fn with_deadline(mut self, deadline: impl Into<Option<Duration>>) -> Limits {
        self.deadline = deadline.into();
        self
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::DEFAULT
    }
}

thread_local! {
    /// The limits the render running on this thread is under.
    ///
    /// Template functions are plain `fn` pointers with no room for a context
    /// argument, so a limit they have to honour cannot be passed to them. It is put
    /// here for the length of the render instead.
    static ACTIVE: Cell<Limits> = const { Cell::new(Limits::DEFAULT) };

    /// The instant [`Limits::deadline`] falls on, rather than the span itself, so
    /// every check is a comparison and not an arithmetic.
    static EXPIRES: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// The limits the render on this thread is running under.
///
/// A custom function that turns a number from the template into an allocation should
/// check it against this, the way the built-in generating functions do -- otherwise
/// `{{ myFunc 9999999999 }}` is a way to end the process, whatever the template's
/// limits say.
///
/// Valid only on the thread the render runs on, synchronously inside it: a function
/// that spawns a thread, or code outside any render, reads [`Limits::default()`] --
/// the safe defaults, never the calling render's limits. How the value gets here is
/// not part of the API.
///
/// A nested render -- a custom function rendering a template of its own -- runs under
/// that inner template's limits, with its own budgets, and the outer render's come
/// back when it finishes.
pub fn active() -> Limits {
    ACTIVE.get()
}

/// How much of the current render's [`deadline`](Limits::deadline) is left, or `None`
/// when it has none.
///
/// A custom function with work it can do in pieces should look here, the way one that
/// allocates looks at [`active`]. A function that simply runs to its end needs
/// nothing: the engine checks the deadline before it calls one.
///
/// `Some(Duration::ZERO)` means the deadline has passed. The render will stop at the
/// next check either way, so a function that sees it may as well return.
pub fn remaining() -> Option<Duration> {
    EXPIRES
        .get()
        .map(|expires| expires.saturating_duration_since(Instant::now()))
}

/// The instant the current render's deadline falls on.
pub(crate) fn expires() -> Option<Instant> {
    EXPIRES.get()
}

/// Installs limits for the length of a render, restoring the previous ones on drop.
///
/// Restoring rather than resetting is what makes a render inside a template function
/// safe: the inner one gets its own limits and the outer one gets its own back.
pub(crate) struct Active {
    previous: Limits,
    previous_expires: Option<Instant>,
}

impl Active {
    pub(crate) fn install(limits: Limits) -> Active {
        // `checked_add` rather than `+`: adding a `Duration` to an `Instant` panics on
        // overflow, and the duration comes from the caller. A deadline so far off that
        // no clock can represent it is no deadline, which is the honest reading of it.
        let expires = limits
            .deadline
            .and_then(|deadline| Instant::now().checked_add(deadline));
        Active {
            previous: ACTIVE.replace(limits),
            previous_expires: EXPIRES.replace(expires),
        }
    }
}

impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.set(self.previous);
        EXPIRES.set(self.previous_expires);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_constant() {
        assert_eq!(Limits::default(), Limits::DEFAULT);
        assert_eq!(active(), Limits::DEFAULT);
    }

    #[test]
    fn a_deadline_is_turned_into_an_instant_and_put_back() {
        assert_eq!(remaining(), None);
        {
            let _active = Active::install(Limits::default().with_deadline(Duration::from_secs(60)));
            let left = remaining().expect("a deadline is in force");
            assert!(left > Duration::from_secs(50), "{left:?}");
        }
        assert_eq!(remaining(), None, "restored on drop");
    }

    /// A span no clock can add to `now` is treated as no deadline rather than panicking
    /// on the overflow, since the span comes from the caller.
    #[test]
    fn an_unrepresentable_deadline_is_no_deadline() {
        let _active = Active::install(Limits::default().with_deadline(Duration::MAX));
        assert_eq!(remaining(), None);
    }

    /// Each builder replaces its one field and keeps the other four, whatever the
    /// starting point.
    #[test]
    fn the_builders_replace_one_field_and_keep_the_rest() {
        let limits = Limits::default()
            .with_iterations(7)
            .with_total_iterations(8)
            .with_output_bytes(9)
            .with_function_output(None)
            .with_deadline(Duration::from_secs(1));
        assert_eq!(limits.iterations, Some(7));
        assert_eq!(limits.total_iterations, Some(8));
        assert_eq!(limits.output_bytes, Some(9));
        assert_eq!(limits.function_output, None);
        assert_eq!(limits.deadline, Some(Duration::from_secs(1)));

        let lifted = limits.with_iterations(None);
        assert_eq!(lifted.iterations, None);
        assert_eq!(lifted.total_iterations, Some(8), "the rest is untouched");
        assert_eq!(lifted.with_total_iterations(None).total_iterations, None);
        assert_eq!(lifted.with_output_bytes(None).output_bytes, None);
        assert_eq!(
            Limits::unlimited().with_deadline(None),
            Limits::unlimited(),
            "None is expressible too"
        );
    }

    #[test]
    fn installing_restores_the_previous_limits() {
        let outer = active();
        {
            let _inner = Active::install(Limits::unlimited());
            assert_eq!(active(), Limits::unlimited());
            {
                let _nested = Active::install(Limits::DEFAULT);
                assert_eq!(active(), Limits::DEFAULT);
            }
            assert_eq!(active(), Limits::unlimited());
        }
        assert_eq!(active(), outer);
    }
}
