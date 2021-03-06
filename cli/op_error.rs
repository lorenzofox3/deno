// Copyright 2018-2020 the Deno authors. All rights reserved. MIT license.

//! There are many types of errors in Deno:
//! - ErrBox: a generic boxed object. This is the super type of all
//!   errors handled in Rust.
//! - JSError: exceptions thrown from V8 into Rust. Usually a user exception.
//!   These are basically a big JSON structure which holds information about
//!   line numbers. We use this to pretty-print stack traces. These are
//!   never passed back into the runtime.
//! - OpError: these are errors that happen during ops, which are passed
//!   back into the runtime, where an exception object is created and thrown.
//!   OpErrors have an integer code associated with them - access this via the `kind` field.
//! - Diagnostic: these are errors that originate in TypeScript's compiler.
//!   They're similar to JSError, in that they have line numbers.
//!   But Diagnostics are compile-time type errors, whereas JSErrors are runtime exceptions.
//!
//! TODO:
//! - rename/merge JSError with V8Exception?

use crate::import_map::ImportMapError;
use deno_core::ErrBox;
use deno_core::ModuleResolutionError;
use dlopen;
use reqwest;
use rustyline::error::ReadlineError;
use std;
use std::env::VarError;
use std::error::Error;
use std::fmt;
use std::io;
use url;

// Warning! The values in this enum are duplicated in js/errors.ts
// Update carefully!
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ErrorKind {
  NotFound = 1,
  PermissionDenied = 2,
  ConnectionRefused = 3,
  ConnectionReset = 4,
  ConnectionAborted = 5,
  NotConnected = 6,
  AddrInUse = 7,
  AddrNotAvailable = 8,
  BrokenPipe = 9,
  AlreadyExists = 10,
  InvalidData = 13,
  TimedOut = 14,
  Interrupted = 15,
  WriteZero = 16,
  UnexpectedEof = 17,
  BadResource = 18,
  Http = 19,
  URIError = 20,
  TypeError = 21,
  /// This maps to window.Error - ie. a generic error type
  /// if no better context is available.
  /// https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Error
  Other = 22,
}

#[derive(Debug)]
pub struct OpError {
  pub kind: ErrorKind,
  pub msg: String,
}

impl OpError {
  fn new(kind: ErrorKind, msg: String) -> Self {
    Self { kind, msg }
  }

  pub fn not_found(msg: String) -> Self {
    Self::new(ErrorKind::NotFound, msg)
  }

  pub fn other(msg: String) -> Self {
    Self::new(ErrorKind::Other, msg)
  }

  pub fn type_error(msg: String) -> Self {
    Self::new(ErrorKind::TypeError, msg)
  }

  pub fn http(msg: String) -> Self {
    Self::new(ErrorKind::Http, msg)
  }

  pub fn uri_error(msg: String) -> Self {
    Self::new(ErrorKind::URIError, msg)
  }

  pub fn permission_denied(msg: String) -> OpError {
    Self::new(ErrorKind::PermissionDenied, msg)
  }

  pub fn bad_resource() -> OpError {
    Self::new(ErrorKind::BadResource, "bad resource id".to_string())
  }
}

impl Error for OpError {}

impl fmt::Display for OpError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.pad(self.msg.as_str())
  }
}

impl From<ImportMapError> for OpError {
  fn from(error: ImportMapError) -> Self {
    OpError::from(&error)
  }
}

impl From<&ImportMapError> for OpError {
  fn from(error: &ImportMapError) -> Self {
    Self {
      kind: ErrorKind::Other,
      msg: error.to_string(),
    }
  }
}

impl From<ModuleResolutionError> for OpError {
  fn from(error: ModuleResolutionError) -> Self {
    OpError::from(&error)
  }
}

impl From<&ModuleResolutionError> for OpError {
  fn from(error: &ModuleResolutionError) -> Self {
    Self {
      kind: ErrorKind::URIError,
      msg: error.to_string(),
    }
  }
}

impl From<VarError> for OpError {
  fn from(error: VarError) -> Self {
    OpError::from(&error)
  }
}

impl From<&VarError> for OpError {
  fn from(error: &VarError) -> Self {
    use VarError::*;
    let kind = match error {
      NotPresent => ErrorKind::NotFound,
      NotUnicode(..) => ErrorKind::InvalidData,
    };

    Self {
      kind,
      msg: error.to_string(),
    }
  }
}

impl From<io::Error> for OpError {
  fn from(error: io::Error) -> Self {
    OpError::from(&error)
  }
}

impl From<&io::Error> for OpError {
  fn from(error: &io::Error) -> Self {
    use io::ErrorKind::*;
    let kind = match error.kind() {
      NotFound => ErrorKind::NotFound,
      PermissionDenied => ErrorKind::PermissionDenied,
      ConnectionRefused => ErrorKind::ConnectionRefused,
      ConnectionReset => ErrorKind::ConnectionReset,
      ConnectionAborted => ErrorKind::ConnectionAborted,
      NotConnected => ErrorKind::NotConnected,
      AddrInUse => ErrorKind::AddrInUse,
      AddrNotAvailable => ErrorKind::AddrNotAvailable,
      BrokenPipe => ErrorKind::BrokenPipe,
      AlreadyExists => ErrorKind::AlreadyExists,
      InvalidInput => ErrorKind::TypeError,
      InvalidData => ErrorKind::InvalidData,
      TimedOut => ErrorKind::TimedOut,
      Interrupted => ErrorKind::Interrupted,
      WriteZero => ErrorKind::WriteZero,
      UnexpectedEof => ErrorKind::UnexpectedEof,
      Other => ErrorKind::Other,
      WouldBlock => unreachable!(),
      // Non-exhaustive enum - might add new variants
      // in the future
      _ => unreachable!(),
    };

    Self {
      kind,
      msg: error.to_string(),
    }
  }
}

impl From<url::ParseError> for OpError {
  fn from(error: url::ParseError) -> Self {
    OpError::from(&error)
  }
}

impl From<&url::ParseError> for OpError {
  fn from(error: &url::ParseError) -> Self {
    Self {
      kind: ErrorKind::URIError,
      msg: error.to_string(),
    }
  }
}
impl From<reqwest::Error> for OpError {
  fn from(error: reqwest::Error) -> Self {
    OpError::from(&error)
  }
}

impl From<&reqwest::Error> for OpError {
  fn from(error: &reqwest::Error) -> Self {
    match error.source() {
      Some(err_ref) => None
        .or_else(|| {
          err_ref
            .downcast_ref::<url::ParseError>()
            .map(|e| e.clone().into())
        })
        .or_else(|| {
          err_ref
            .downcast_ref::<io::Error>()
            .map(|e| e.to_owned().into())
        })
        .or_else(|| {
          err_ref
            .downcast_ref::<serde_json::error::Error>()
            .map(|e| e.into())
        })
        .unwrap_or_else(|| Self {
          kind: ErrorKind::Http,
          msg: error.to_string(),
        }),
      None => Self {
        kind: ErrorKind::Http,
        msg: error.to_string(),
      },
    }
  }
}

impl From<ReadlineError> for OpError {
  fn from(error: ReadlineError) -> Self {
    OpError::from(&error)
  }
}

impl From<&ReadlineError> for OpError {
  fn from(error: &ReadlineError) -> Self {
    use ReadlineError::*;
    let kind = match error {
      Io(err) => return err.into(),
      Eof => ErrorKind::UnexpectedEof,
      Interrupted => ErrorKind::Interrupted,
      #[cfg(unix)]
      Errno(err) => return err.into(),
      _ => unimplemented!(),
    };

    Self {
      kind,
      msg: error.to_string(),
    }
  }
}

impl From<serde_json::error::Error> for OpError {
  fn from(error: serde_json::error::Error) -> Self {
    OpError::from(&error)
  }
}

impl From<&serde_json::error::Error> for OpError {
  fn from(error: &serde_json::error::Error) -> Self {
    use serde_json::error::*;
    let kind = match error.classify() {
      Category::Io => ErrorKind::TypeError,
      Category::Syntax => ErrorKind::TypeError,
      Category::Data => ErrorKind::InvalidData,
      Category::Eof => ErrorKind::UnexpectedEof,
    };

    Self {
      kind,
      msg: error.to_string(),
    }
  }
}

#[cfg(unix)]
mod unix {
  use super::{ErrorKind, OpError};
  use nix::errno::Errno::*;
  pub use nix::Error;
  use nix::Error::Sys;

  impl From<Error> for OpError {
    fn from(error: Error) -> Self {
      OpError::from(&error)
    }
  }

  impl From<&Error> for OpError {
    fn from(error: &Error) -> Self {
      let kind = match error {
        Sys(EPERM) => ErrorKind::PermissionDenied,
        Sys(EINVAL) => ErrorKind::TypeError,
        Sys(ENOENT) => ErrorKind::NotFound,
        Sys(UnknownErrno) => unreachable!(),
        Sys(_) => unreachable!(),
        Error::InvalidPath => ErrorKind::TypeError,
        Error::InvalidUtf8 => ErrorKind::InvalidData,
        Error::UnsupportedOperation => unreachable!(),
      };

      Self {
        kind,
        msg: error.to_string(),
      }
    }
  }
}

impl From<dlopen::Error> for OpError {
  fn from(error: dlopen::Error) -> Self {
    OpError::from(&error)
  }
}

impl From<&dlopen::Error> for OpError {
  fn from(error: &dlopen::Error) -> Self {
    use dlopen::Error::*;
    let kind = match error {
      NullCharacter(_) => ErrorKind::Other,
      OpeningLibraryError(e) => return e.into(),
      SymbolGettingError(e) => return e.into(),
      AddrNotMatchingDll(e) => return e.into(),
      NullSymbol => ErrorKind::Other,
    };

    Self {
      kind,
      msg: error.to_string(),
    }
  }
}

impl From<ErrBox> for OpError {
  fn from(error: ErrBox) -> Self {
    #[cfg(unix)]
    fn unix_error_kind(err: &ErrBox) -> Option<OpError> {
      err.downcast_ref::<unix::Error>().map(|e| e.into())
    }

    #[cfg(not(unix))]
    fn unix_error_kind(_: &ErrBox) -> Option<OpError> {
      None
    }

    None
      .or_else(|| {
        error
          .downcast_ref::<OpError>()
          .map(|e| OpError::new(e.kind, e.msg.to_string()))
      })
      .or_else(|| error.downcast_ref::<reqwest::Error>().map(|e| e.into()))
      .or_else(|| error.downcast_ref::<ImportMapError>().map(|e| e.into()))
      .or_else(|| error.downcast_ref::<io::Error>().map(|e| e.into()))
      .or_else(|| {
        error
          .downcast_ref::<ModuleResolutionError>()
          .map(|e| e.into())
      })
      .or_else(|| error.downcast_ref::<url::ParseError>().map(|e| e.into()))
      .or_else(|| error.downcast_ref::<VarError>().map(|e| e.into()))
      .or_else(|| error.downcast_ref::<ReadlineError>().map(|e| e.into()))
      .or_else(|| {
        error
          .downcast_ref::<serde_json::error::Error>()
          .map(|e| e.into())
      })
      .or_else(|| error.downcast_ref::<dlopen::Error>().map(|e| e.into()))
      .or_else(|| unix_error_kind(&error))
      .unwrap_or_else(|| {
        panic!("Can't downcast {:?} to OpError", error);
      })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn io_error() -> io::Error {
    io::Error::from(io::ErrorKind::NotFound)
  }

  fn url_error() -> url::ParseError {
    url::ParseError::EmptyHost
  }

  fn import_map_error() -> ImportMapError {
    ImportMapError {
      msg: "an import map error".to_string(),
    }
  }

  #[test]
  fn test_simple_error() {
    let err = OpError::not_found("foo".to_string());
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert_eq!(err.to_string(), "foo");
  }

  #[test]
  fn test_io_error() {
    let err = OpError::from(io_error());
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert_eq!(err.to_string(), "entity not found");
  }

  #[test]
  fn test_url_error() {
    let err = OpError::from(url_error());
    assert_eq!(err.kind, ErrorKind::URIError);
    assert_eq!(err.to_string(), "empty host");
  }

  // TODO find a way to easily test tokio errors and unix errors

  #[test]
  fn test_import_map_error() {
    let err = OpError::from(import_map_error());
    assert_eq!(err.kind, ErrorKind::Other);
    assert_eq!(err.to_string(), "an import map error");
  }

  #[test]
  fn test_bad_resource() {
    let err = OpError::bad_resource();
    assert_eq!(err.kind, ErrorKind::BadResource);
    assert_eq!(err.to_string(), "bad resource id");
  }
  #[test]
  fn test_permission_denied() {
    let err = OpError::permission_denied(
      "run again with the --allow-net flag".to_string(),
    );
    assert_eq!(err.kind, ErrorKind::PermissionDenied);
    assert_eq!(err.to_string(), "run again with the --allow-net flag");
  }
}
