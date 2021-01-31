// TODO implement a tui logger.
use simplelog::*;

pub fn init() {
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
    )])
    .unwrap();
}
