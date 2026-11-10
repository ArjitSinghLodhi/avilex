use std::borrow::Cow;
use std::error::Error;
use std::fmt;

use avilex_macros::Resource;

pub type SystemErrorHandlerFn = fn(error: RunSystemError);

#[derive(Resource)]
pub struct SystemErrorHandler(pub SystemErrorHandlerFn);

impl SystemErrorHandler {
    pub fn new(handler: fn(error: RunSystemError)) -> Self {
        Self(handler)
    }

    pub fn handler(&self) -> fn(error: RunSystemError) {
        self.0
    }
}

pub fn default_system_error_handler(error: RunSystemError) {
    match error {
        RunSystemError::Skipped(err) => {
            if err.severity() == Severity::Panic {
                panic!("{}", err)
            }
        }
        RunSystemError::Failed(fail_error) => {
            if fail_error.severity() == Severity::Panic {
                panic!("{}", fail_error)
            }
        }
    }
}

#[derive(Debug)]
pub enum RunSystemError {
    Skipped(SystemParamError),
    Failed(SystemFailError),
}

/// Contextual information for why a system parameter could not be extracted from the world.
#[derive(Debug)]
pub struct SystemParamError {
    param_name: Cow<'static, str>,
    message: Cow<'static, str>,
    severity: Severity,
    is_skipped: bool,
}

#[derive(Debug)]
pub struct SystemFailError {
    system_name: Cow<'static, str>,
    error: Box<dyn Error + Send + Sync>,
    context: Cow<'static, str>,
    severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum Severity {
    Ignore,
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Panic,
}

impl RunSystemError {
    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped(_))
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    pub fn skipped(self) -> Option<SystemParamError> {
        match self {
            Self::Skipped(err) => Some(err),
            _ => None,
        }
    }

    pub fn failed(self) -> Option<SystemFailError> {
        match self {
            Self::Failed(err) => Some(err),
            _ => None,
        }
    }

    pub fn as_skipped(&self) -> Option<&SystemParamError> {
        match self {
            Self::Skipped(err) => Some(err),
            _ => None,
        }
    }

    pub fn as_failed(&self) -> Option<&SystemFailError> {
        match self {
            Self::Failed(err) => Some(err),
            _ => None,
        }
    }
}

impl SystemParamError {
    pub fn new(
        param_name: impl Into<Cow<'static, str>>,
        severity: Severity,
        message: impl Into<Cow<'static, str>>,
        is_skipped: bool,
    ) -> Self {
        Self {
            param_name: param_name.into(),
            severity,
            message: message.into(),
            is_skipped,
        }
    }

    pub fn param_name(&self) -> &str {
        self.param_name.as_ref()
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn message(&self) -> &str {
        self.message.as_ref()
    }

    pub fn is_skipped(&self) -> bool {
        self.is_skipped
    }

    pub fn set_param_name(&mut self, param_name: impl Into<Cow<'static, str>>) {
        self.param_name = param_name.into();
    }

    pub fn set_severity(&mut self, severity: Severity) {
        self.severity = severity;
    }

    pub fn set_message(&mut self, message: impl Into<Cow<'static, str>>) {
        self.message = message.into();
    }

    pub fn set_is_skipped(&mut self, is_skipped: bool) {
        self.is_skipped = is_skipped;
    }

    pub fn with_param_name(mut self, param_name: impl Into<Cow<'static, str>>) -> Self {
        self.param_name = param_name.into();
        self
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.message = message.into();
        self
    }

    pub fn with_is_skipped(mut self, is_skipped: bool) -> Self {
        self.is_skipped = is_skipped;
        self
    }
}

impl SystemFailError {
    pub fn new(
        system_name: impl Into<Cow<'static, str>>,
        error: impl Into<Box<dyn Error + Send + Sync>>,
        context: impl Into<Cow<'static, str>>,
        severity: Severity,
    ) -> Self {
        Self {
            system_name: system_name.into(),
            error: error.into(),
            context: context.into(),
            severity,
        }
    }

    pub fn system_name(&self) -> &str {
        self.system_name.as_ref()
    }

    pub fn error(&self) -> &(dyn Error + Send + Sync) {
        self.error.as_ref()
    }

    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn set_system_name(&mut self, system_name: impl Into<Cow<'static, str>>) {
        self.system_name = system_name.into();
    }

    pub fn set_error(&mut self, error: impl Into<Box<dyn Error + Send + Sync>>) {
        self.error = error.into();
    }

    pub fn set_context(&mut self, context: impl Into<Cow<'static, str>>) {
        self.context = context.into();
    }

    pub fn set_severity(&mut self, severity: Severity) {
        self.severity = severity;
    }

    pub fn with_system_name(mut self, system_name: impl Into<Cow<'static, str>>) -> Self {
        self.system_name = system_name.into();
        self
    }

    pub fn with_error(mut self, error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        self.error = error.into();
        self
    }

    pub fn with_context(mut self, context: impl Into<Cow<'static, str>>) -> Self {
        self.context = context.into();
        self
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }
}

impl From<SystemParamError> for RunSystemError {
    fn from(err: SystemParamError) -> Self {
        Self::Skipped(err)
    }
}

impl From<SystemFailError> for RunSystemError {
    fn from(err: SystemFailError) -> Self {
        Self::Failed(err)
    }
}

impl fmt::Display for RunSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Skipped(err) => write!(f, "SystemSkipped\n{}", err),
            Self::Failed(err) => write!(f, "{}", err),
        }
    }
}

impl Error for RunSystemError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Skipped(err) => Some(err),
            Self::Failed(err) => Some(err),
        }
    }
}

impl fmt::Display for SystemParamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.is_skipped { "Skipped" } else { "Failed" };
        write!(
            f,
            "SystemParamError\n\
            SystemParam '{}' {} to run\n\
            Severity: {:?}\n\
            Message: {}",
            self.param_name(),
            status,
            self.severity(),
            self.message()
        )
    }
}

impl Error for SystemParamError {}

impl fmt::Display for SystemFailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SystemFailError\n\
            System Name: {}\n\
            Context: {}\n\
            Error: {}",
            self.system_name(),
            self.context(),
            self.error()
        )
    }
}

impl Error for SystemFailError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}
