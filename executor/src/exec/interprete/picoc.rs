use crate::exec::interprete::bind;
use crate::exec::interprete::bind::{picoc_clean, picoc_execute, picoc_init, picoc_insluce_header};

// ISSUE: Without boxed, picoc crash
pub struct Picoc(Box<bind::PicocWrapper>);

impl Default for Picoc {
    fn default() -> Self {
        let mut pc = Box::new(bind::PicocWrapper::default());
        picoc_init(&mut pc);
        picoc_insluce_header(&mut pc);
        Self(pc)
    }
}

impl Picoc {
    pub fn execute(&mut self, p: String) -> bool {
        picoc_execute(&mut self.0, p)
    }
}

impl Drop for Picoc {
    fn drop(&mut self) {
        picoc_clean(&mut self.0);
    }
}
