//! Error type that may be raised while parsing

use crate::parse::Span;
use std::error;
use std::fmt;

#[derive(Debug, PartialEq)]
pub struct Error<'a> {
    pub span: Span<'a>,
    pub expect: String,
    pub found: String,
    pub context: Option<String>,
}

impl<'a> Error<'a> {
    pub fn add_context(source: Self, context: String) -> Self {
        let context = if let Some(old_context) = source.context {
            format!("{}: {}", old_context, context)
        } else {
            context
        };

        Self {
            context: Some(context),
            ..source
        }
    }
}

impl<'a> error::Error for Error<'a> {}

impl<'a> fmt::Display for Error<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}
