use std::borrow::Cow;
use std::error::Error;
use std::fmt;

use avilex_macros::Resource;

/// Function pointer type used by [`SystemErrorHandler`] to handle a [`RunSystemError`].
///
/// [`SystemErrorHandler`]: crate::app::system::system_error::SystemErrorHandler
/// [`RunSystemError`]: crate::app::system::system_error::RunSystemError
pub type SystemErrorHandlerFn = fn(error: RunSystemError);

/// A [`Resource`] wrapping the callback invoked whenever a system fails or is skipped during execution.
///
/// Insert this into [`App`] or [`World`] to override [`default_system_error_handler`].
///
/// [`Resource`]: crate::ecs::resources::Resource
/// [`App`]: crate::app::App
/// [`World`]: crate::ecs::world::World
/// [`default_system_error_handler`]: crate::app::system::system_error::default_system_error_handler
#[derive(Resource)]
pub struct SystemErrorHandler(pub SystemErrorHandlerFn);

/// The fallback handler used when no [`SystemErrorHandler`] resource is inserted.
///
/// Panics if the error's [`Severity`] is [`Severity::Panic`], and ignores all lower severities.
///
/// [`SystemErrorHandler`]: crate::app::system::system_error::SystemErrorHandler
/// [`Severity`]: crate::app::system::system_error::Severity
/// [`Severity::Panic`]: crate::app::system::system_error::Severity::Panic
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

/// An error produced when a system is skipped or fails to run.
#[derive(Debug)]
pub enum RunSystemError {
    /// The system was skipped because a parameter returned a [`SystemParamError`] with `is_skipped` set to `true`.
    ///
    /// [`SystemParamError`]: crate::app::system::system_error::SystemParamError
    Skipped(SystemParamError),
    /// The system failed either because a non-skipped [`SystemParamError`] occurred or the system itself returned an error.
    ///
    /// [`SystemParamError`]: crate::app::system::system_error::SystemParamError
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

/// Contextual information for a system failure caused by either a non-skipped parameter error or a fallible system error.
#[derive(Debug)]
pub struct SystemFailError {
    system_name: Cow<'static, str>,
    error: Box<dyn Error + Send + Sync>,
    context: Cow<'static, str>,
    severity: Severity,
}

/// The severity level attached to a [`RunSystemError`].
///
/// [`RunSystemError`]: crate::app::system::system_error::RunSystemError
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum Severity {
    /// Completely silent; can be safely ignored.
    Ignore,
    /// Fine-grained diagnostic trace output.
    Trace,
    /// Diagnostic debugging information.
    Debug,
    /// Informational message.
    Info,
    /// Non-fatal warning condition.
    Warning,
    /// Recoverable or reported runtime error.
    Error,
    /// Unrecoverable error that triggers a panic.
    Panic,
}

impl RunSystemError {
    /// Returns `true` if this error is [`RunSystemError::Skipped`].
    ///
    /// [`RunSystemError::Skipped`]: Self::Skipped
    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped(_))
    }

    /// Returns `true` if this error is [`RunSystemError::Failed`].
    ///
    /// [`RunSystemError::Failed`]: Self::Failed
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// Consumes `self` and returns the inner [`SystemParamError`] if this is [`RunSystemError::Skipped`].
    ///
    /// [`SystemParamError`]: crate::app::system::system_error::SystemParamError
    /// [`RunSystemError::Skipped`]: Self::Skipped
    pub fn skipped(self) -> Option<SystemParamError> {
        match self {
            Self::Skipped(err) => Some(err),
            _ => None,
        }
    }

    /// Consumes `self` and returns the inner [`SystemFailError`] if this is [`RunSystemError::Failed`].
    ///
    /// [`SystemFailError`]: crate::app::system::system_error::SystemFailError
    /// [`RunSystemError::Failed`]: Self::Failed
    pub fn failed(self) -> Option<SystemFailError> {
        match self {
            Self::Failed(err) => Some(err),
            _ => None,
        }
    }

    /// Returns a shared reference to the inner [`SystemParamError`] if this is [`RunSystemError::Skipped`].
    ///
    /// [`SystemParamError`]: crate::app::system::system_error::SystemParamError
    /// [`RunSystemError::Skipped`]: Self::Skipped
    pub fn as_skipped(&self) -> Option<&SystemParamError> {
        match self {
            Self::Skipped(err) => Some(err),
            _ => None,
        }
    }

    /// Returns a shared reference to the inner [`SystemFailError`] if this is [`RunSystemError::Failed`].
    ///
    /// [`SystemFailError`]: crate::app::system::system_error::SystemFailError
    /// [`RunSystemError::Failed`]: Self::Failed
    pub fn as_failed(&self) -> Option<&SystemFailError> {
        match self {
            Self::Failed(err) => Some(err),
            _ => None,
        }
    }
}

impl SystemParamError {
    /// Creates a new [`SystemParamError`].
    ///
    /// [`SystemParamError`]: Self
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

    /// Returns the name of the system parameter that triggered this error.
    pub fn param_name(&self) -> &str {
        self.param_name.as_ref()
    }

    /// Returns the [`Severity`] of this error.
    ///
    /// [`Severity`]: crate::app::system::system_error::Severity
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Returns the descriptive error message.
    pub fn message(&self) -> &str {
        self.message.as_ref()
    }

    /// Returns `true` if the system was skipped rather than treated as a failure.
    pub fn is_skipped(&self) -> bool {
        self.is_skipped
    }

    /// Updates the parameter name.
    pub fn set_param_name(&mut self, param_name: impl Into<Cow<'static, str>>) {
        self.param_name = param_name.into();
    }

    /// Updates the [`Severity`].
    ///
    /// [`Severity`]: crate::app::system::system_error::Severity
    pub fn set_severity(&mut self, severity: Severity) {
        self.severity = severity;
    }

    /// Updates the descriptive error message.
    pub fn set_message(&mut self, message: impl Into<Cow<'static, str>>) {
        self.message = message.into();
    }

    /// Updates whether the system was skipped.
    pub fn set_is_skipped(&mut self, is_skipped: bool) {
        self.is_skipped = is_skipped;
    }

    /// Builder method that sets the parameter name.
    pub fn with_param_name(mut self, param_name: impl Into<Cow<'static, str>>) -> Self {
        self.param_name = param_name.into();
        self
    }

    /// Builder method that sets the [`Severity`].
    ///
    /// [`Severity`]: crate::app::system::system_error::Severity
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Builder method that sets the descriptive error message.
    pub fn with_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.message = message.into();
        self
    }

    /// Builder method that sets whether the system was skipped.
    pub fn with_is_skipped(mut self, is_skipped: bool) -> Self {
        self.is_skipped = is_skipped;
        self
    }
}

impl SystemFailError {
    /// Creates a new [`SystemFailError`].
    ///
    /// [`SystemFailError`]: Self
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

    /// Returns the name of the system that failed.
    pub fn system_name(&self) -> &str {
        self.system_name.as_ref()
    }

    /// Returns a reference to the underlying error value.
    pub fn error(&self) -> &(dyn Error + Send + Sync) {
        self.error.as_ref()
    }

    /// Returns the additional context string attached to the failure.
    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    /// Returns the [`Severity`] of this error.
    ///
    /// [`Severity`]: crate::app::system::system_error::Severity
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Updates the system name.
    pub fn set_system_name(&mut self, system_name: impl Into<Cow<'static, str>>) {
        self.system_name = system_name.into();
    }

    /// Updates the underlying error value.
    pub fn set_error(&mut self, error: impl Into<Box<dyn Error + Send + Sync>>) {
        self.error = error.into();
    }

    /// Updates the context string.
    pub fn set_context(&mut self, context: impl Into<Cow<'static, str>>) {
        self.context = context.into();
    }

    /// Updates the [`Severity`].
    ///
    /// [`Severity`]: crate::app::system::system_error::Severity
    pub fn set_severity(&mut self, severity: Severity) {
        self.severity = severity;
    }

    /// Builder method that sets the system name.
    pub fn with_system_name(mut self, system_name: impl Into<Cow<'static, str>>) -> Self {
        self.system_name = system_name.into();
        self
    }

    /// Builder method that sets the underlying error value.
    pub fn with_error(mut self, error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        self.error = error.into();
        self
    }

    /// Builder method that sets the context string.
    pub fn with_context(mut self, context: impl Into<Cow<'static, str>>) -> Self {
        self.context = context.into();
        self
    }

    /// Builder method that sets the [`Severity`].
    ///
    /// [`Severity`]: crate::app::system::system_error::Severity
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
