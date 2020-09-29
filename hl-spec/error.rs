//! Error type that may be raised while parsing

use crate::parse::Span;
use ansi_term::Color::Red;

#[derive(Debug, PartialEq, Default)]
pub struct ParseError {
    pub err_snippets: String,
    pub location: (usize, usize),
    pub expect: String,
    pub found: String,
    pub context: Vec<String>,
    pub filename: Option<String>,
}

impl ParseError {
    pub fn new(span: Span) -> Self {
        let err_line = span.fragment().lines().next().unwrap_or("EOF");
        Self {
            err_snippets: err_line.to_string(),
            location: (span.location_line() as usize, span.location_offset()),
            ..Default::default()
        }
    }

    pub fn err_snippets<T: Into<String>>(mut self, snippets: T) -> Self {
        self.err_snippets = snippets.into();
        self
    }

    pub fn expect<T: Into<String>>(mut self, msg: T) -> Self {
        self.expect = msg.into();
        self
    }

    pub fn found<T: Into<String>>(mut self, msg: T) -> Self {
        self.found = msg.into();
        self
    }

    pub fn add_context<T: Into<String>>(mut self, ctx: T) -> Self {
        self.context.push(ctx.into());
        self
    }

    pub fn filename<T: Into<String>>(mut self, f: T) -> Self {
        self.filename = Some(f.into());
        self
    }

    pub fn row(mut self, r: usize) -> Self {
        self.location.0 = r;
        self
    }

    pub fn column(mut self, c: usize) -> Self {
        self.location.1 = c;
        self
    }
}

impl Into<nom::Err<ParseError>> for ParseError {
    fn into(self) -> nom::Err<ParseError> {
        nom::Err::Error(self)
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(filename) = self.filename.as_ref() {
            write!(f, "In {}:", filename)?;
        }
        write!(
            f,
            "{}:{}: {}: ",
            self.location.0,
            self.location.1,
            Red.paint("ERROR")
        )?;
        for ctx in self.context.iter() {
            write!(f, "in {}: ", ctx)?;
        }
        writeln!(f, "expect {}, but found {}.", self.expect, self.found)?;
        writeln!(f, "\t{}", self.err_snippets)?;
        write!(f, "\t")?;
        for _ in 0..self.location.1 {
            write!(f, " ")?;
        }
        write!(f, "{}", Red.paint("^^"))
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_fmt() {
        let span = Span::new("fn foo()\n");
        let err = ParseError::new(span)
            .column(1)
            .filename("test.hl")
            .add_context("test")
            .expect("test")
            .found("test");

        assert_eq!(
            "In test.hl:1:0: ERROR: in test: expect test, but found test.\n\tfn foo()",
            err.to_string()
        );
    }
}
