use tokio::process::Command;

#[derive(Clone, Debug)]
pub struct App {
    pub bin: String,
    pub args: Vec<Arg>,
}

impl App {
    pub fn new(bin: &str) -> Self {
        Self {
            bin: bin.to_string(),
            args: vec![],
        }
    }

    pub fn arg(&mut self, a: Arg) -> &mut Self {
        self.args.push(a);
        self
    }

    pub fn args<'a, T: IntoIterator<Item = &'a Arg>>(&mut self, a: T) -> &mut Self {
        self.args.extend(a.into_iter().cloned());
        self
    }

    pub fn into_cmd(self) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.args(self.iter_arg());
        cmd
    }

    pub fn iter_arg(mut self) -> impl Iterator<Item = String> {
        self.args.reverse();
        IterArg {
            args: self.args,
            state: ArgState::Start,
            arg_vals: None,
        }
    }
}

struct IterArg {
    args: Vec<Arg>,
    state: ArgState,
    arg_vals: Option<OptVal>,
}

enum ArgState {
    Start,
    NameOut,
    ReadingVal,
}

impl Iterator for IterArg {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.state {
                ArgState::Start => {
                    return if let Some(arg) = self.args.pop() {
                        match arg {
                            Arg::Flag(val) => Some(val),
                            Arg::Option { name, val } => {
                                self.arg_vals = Some(val);
                                self.state = ArgState::NameOut;
                                Some(name)
                            }
                        }
                    } else {
                        None
                    };
                }
                ArgState::NameOut => match self.arg_vals.take() {
                    Some(opt_val) => match opt_val {
                        OptVal::Normal(val) => {
                            self.state = ArgState::Start;
                            return Some(val);
                        }
                        OptVal::Multiple { sp, mut vals } => {
                            if let Some(sp) = sp {
                                let sp = format!("{}", sp);
                                let val = vals.join(&sp);
                                self.state = ArgState::Start;
                                return Some(val);
                            } else if let Some(val) = vals.pop() {
                                self.state = ArgState::ReadingVal;
                                self.arg_vals = Some(OptVal::multiple(vals, None));
                                return Some(val);
                            } else {
                                self.state = ArgState::Start;
                                continue;
                            }
                        }
                    },
                    _ => panic!(),
                },
                ArgState::ReadingVal => {
                    if let Some(opt_val) = self.arg_vals.take() {
                        if let OptVal::Multiple { mut vals, .. } = opt_val {
                            if let Some(val) = vals.pop() {
                                self.arg_vals = Some(OptVal::multiple(vals, None));
                                return Some(val);
                            } else {
                                self.state = ArgState::Start;
                                continue;
                            }
                        } else {
                            panic!()
                        }
                    } else {
                        panic!()
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Arg {
    Flag(String),
    Option { name: String, val: OptVal },
}

#[derive(Clone, Debug)]
pub enum OptVal {
    Normal(String),
    Multiple { vals: Vec<String>, sp: Option<char> },
}

impl OptVal {
    pub fn normal(val: &str) -> Self {
        Self::Normal(val.to_string())
    }

    pub fn multiple<S: ToString, T: IntoIterator<Item = S>>(vals: T, sp: Option<char>) -> Self {
        Self::Multiple {
            vals: vals.into_iter().map(|v| v.to_string()).collect(),
            sp,
        }
    }
}

impl Arg {
    pub fn new_flag(f: &str) -> Self {
        Self::Flag(f.to_string())
    }

    pub fn new_opt(name: &str, val: OptVal) -> Self {
        Self::Option {
            name: name.to_string(),
            val,
        }
    }
}
