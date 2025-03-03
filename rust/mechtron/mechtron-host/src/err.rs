use cosmic_space::err::SpaceErr;
use std::fmt::{Debug, Display, Formatter, Write};
use std::str::Utf8Error;
use std::string::FromUtf8Error;
use std::sync::{MutexGuard, PoisonError};
use std::sync::mpsc::Sender;
use oneshot::RecvError;
use tokio::sync;
use tokio::sync::mpsc::error::SendError;
use wasmer::{CompileError, ExportError, InstantiationError, RuntimeError};
use crate::WasmHostCall;

pub trait HostErr:
    Debug
    + ToString
    + From<CompileError>
    + From<RuntimeError>
    + From<String>
    + From<&'static str>
    + From<Box<bincode::ErrorKind>>
    + From<ExportError>
    + From<tokio::sync::oneshot::error::RecvError>
    + From<Utf8Error>
    + From<FromUtf8Error>
    + From<InstantiationError>
    + From<SpaceErr>
    + Into<SpaceErr>
{
    fn to_space_err(self) -> SpaceErr;
}

#[derive(Debug)]
pub struct DefaultHostErr {
    message: String,
}

impl From<Utf8Error> for DefaultHostErr {
    fn from(e: Utf8Error) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<tokio::sync::mpsc::error::SendError<WasmHostCall>> for DefaultHostErr {
    fn from(err: SendError<WasmHostCall>) -> Self {
        DefaultHostErr {
            message: err.to_string()
        }
    }
}

impl From<oneshot::RecvError> for DefaultHostErr {
    fn from(value: RecvError) -> Self {
        Self {
            message: value.to_string()
        }
    }
}

impl From<PoisonError<std::sync::MutexGuard<'_, std::sync::mpsc::Sender<WasmHostCall>>>> for DefaultHostErr{
    fn from(e: PoisonError<MutexGuard<'_, Sender<WasmHostCall>>>) -> Self {
         DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<FromUtf8Error> for DefaultHostErr {
    fn from(e: FromUtf8Error) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<InstantiationError> for DefaultHostErr {
    fn from(e: InstantiationError) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<SpaceErr> for DefaultHostErr {
    fn from(err: SpaceErr) -> Self {
        DefaultHostErr{
            message: err.to_string()
        }
    }
}

impl Into<SpaceErr> for DefaultHostErr {
    fn into(self) -> SpaceErr {
        SpaceErr::new(500, self.message)
    }
}

impl HostErr for DefaultHostErr {
    fn to_space_err(self) -> SpaceErr {
        SpaceErr::server_error(self.to_string())
    }
}

impl ToString for DefaultHostErr {
    fn to_string(&self) -> String {
        self.message.clone()
    }
}

impl From<CompileError> for DefaultHostErr {
    fn from(e: CompileError) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<Box<bincode::ErrorKind>> for DefaultHostErr {
    fn from(e: Box<bincode::ErrorKind>) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<RuntimeError> for DefaultHostErr {
    fn from(e: RuntimeError) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<&str> for DefaultHostErr {
    fn from(e: &str) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<String> for DefaultHostErr {
    fn from(e: String) -> Self {
        DefaultHostErr { message: e }
    }
}

impl From<ExportError> for DefaultHostErr {
    fn from(e: ExportError) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for DefaultHostErr {
    fn from(e: tokio::sync::oneshot::error::RecvError) -> Self {
        DefaultHostErr {
            message: e.to_string(),
        }
    }
}
