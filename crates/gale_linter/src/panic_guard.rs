//! Panic isolation for linting.
//!
//! A bug in one rule (most often a byte offset that lands inside a multibyte
//! character) must not take the whole run down with it: every other file, and
//! every other rule on the same file, should still be linted and reported.
//! [`catch`] runs a closure, turns a panic into a [`Caught`] value instead of
//! letting it unwind further, and keeps the default panic message and
//! backtrace hint off stderr while it does so.  The caller reports the panic
//! as a problem on the file instead.
//!
//! This relies on panics **unwinding**.  Building with `panic = "abort"` (for
//! example in `[profile.release]`) turns every panic back into a process abort
//! and makes this guard a no-op, so keep the default `panic = "unwind"`.

use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::sync::Once;

/// Where gale's issue tracker lives, for the "please report this" text.
pub const ISSUES_URL: &str = "https://github.com/codebend3r/gale/issues";

thread_local! {
  /// How many [`catch`] calls are active on this thread.  The panic hook only
  /// stays quiet while this is non-zero.
  static GUARD_DEPTH: Cell<usize> = const { Cell::new(0) };
  /// The message and location of the last panic the hook swallowed.
  static LAST_PANIC: RefCell<Option<Caught>> = const { RefCell::new(None) };
}

/// Installs the hook that silences panics inside [`catch`].
static INSTALL_HOOK: Once = Once::new();

/// A panic that [`catch`] stopped from unwinding any further.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caught {
  /// The panic message, e.g. `byte index 12 is not a char boundary; ...`.
  pub message: String,
  /// `file:line:column` of the panic, when the runtime reported one.
  pub location: Option<String>,
}

impl Caught {
  /// The message with its source location appended, for bug reports.
  pub fn describe(&self) -> String {
    match &self.location {
      Some(location) => format!("{} (at {location})", self.message),
      None => self.message.clone(),
    }
  }
}

/// Run `f`, returning its value, or the panic it raised as a [`Caught`].
///
/// Nested calls are fine: the innermost `catch` sees the panic.  While any
/// `catch` is active on the current thread the panic hook records the panic
/// instead of printing it; panics elsewhere still use the default hook.
pub fn catch<T>(f: impl FnOnce() -> T) -> Result<T, Caught> {
  install_hook();
  GUARD_DEPTH.with(|depth| depth.set(depth.get() + 1));
  let outcome = panic::catch_unwind(AssertUnwindSafe(f));
  GUARD_DEPTH.with(|depth| depth.set(depth.get() - 1));
  outcome.map_err(|payload| {
    LAST_PANIC
      .with(|last| last.borrow_mut().take())
      .unwrap_or_else(|| Caught {
        message: payload_message(payload.as_ref()),
        location: None,
      })
  })
}

/// Install the guard-aware panic hook once per process.
///
/// Inside a [`catch`] the hook stores the message and location for `catch` to
/// return; outside one it defers to whichever hook was installed before.
fn install_hook() {
  INSTALL_HOOK.call_once(|| {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
      if GUARD_DEPTH.with(|depth| depth.get()) == 0 {
        previous(info);
        return;
      }
      let caught = Caught {
        message: payload_message(info.payload()),
        location: info
          .location()
          .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())),
      };
      LAST_PANIC.with(|last| *last.borrow_mut() = Some(caught));
    }));
  });
}

/// The text of a panic payload: the `&str` or `String` passed to `panic!`.
fn payload_message(payload: &(dyn std::any::Any + Send)) -> String {
  if let Some(s) = payload.downcast_ref::<&str>() {
    (*s).to_string()
  } else if let Some(s) = payload.downcast_ref::<String>() {
    s.clone()
  } else {
    "unknown panic".to_string()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn returns_the_value_when_nothing_panics() {
    assert_eq!(catch(|| 2 + 2), Ok(4));
  }

  #[test]
  fn turns_a_panic_into_its_message_and_location() {
    let caught = catch(|| -> usize {
      let s = String::from("é");
      s[1..].len()
    })
    .unwrap_err();
    assert!(
      caught.message.contains("is not a char boundary"),
      "{caught:?}"
    );
    assert!(caught.describe().contains("(at "));
    let location = caught.location.expect("location");
    assert!(location.contains("panic_guard.rs"), "{location}");
  }

  #[test]
  fn nested_catches_see_the_innermost_panic() {
    let outer = catch(|| {
      let inner = catch(|| panic!("inner"));
      assert_eq!(inner.unwrap_err().message, "inner");
      "outer survived"
    });
    assert_eq!(outer, Ok("outer survived"));
  }

  #[test]
  fn formatted_panic_messages_are_kept() {
    let caught = catch(|| panic!("rule {} broke", "x")).unwrap_err();
    assert_eq!(caught.message, "rule x broke");
  }
}
