use healer_core::HashSet;

#[derive(Debug, Clone)]
pub struct Crash;

impl Crash {
    pub fn new() -> Self {
        todo!()
    }

    pub fn with_whitelist(_l: HashSet<String>) -> Self {
        todo!()
    }

    pub fn need_repro(&self) -> bool {
        todo!()
    }

    pub fn record_repro(&self) {
        todo!()
    }
}
