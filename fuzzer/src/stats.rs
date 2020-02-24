use crate::corpus::Corpus;
use crate::feedback::FeedBack;
use crate::report::TestCaseRecord;
use crate::utils::queue::CQueue;
use core::prog::Prog;
use std::sync::Arc;
use std::time::Duration;

pub struct StatSource {
    pub corpus: Arc<Corpus>,
    pub feedback: Arc<FeedBack>,
    pub candidates: Arc<CQueue<Prog>>,
    pub record: Arc<TestCaseRecord>,
}

pub struct Sampler {
    source: StatSource,
    interval: Duration,
    // stats: HashMap<String>,
}
