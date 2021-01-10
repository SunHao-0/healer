//! Error of item/grammar parser.

use crate::parse::Rule;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Parse:`{0}`")]
    Parse(#[from] pest::error::Error<Rule>),
    #[error("Unresolved symbols:{0:?}")]
    Ident(Vec<String>),
}

impl Error {
    pub fn with_idents(idents: Vec<String>) -> Self {
        assert!(!idents.is_empty());
        Error::Ident(idents)
    }
}
