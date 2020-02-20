macro_rules! exits {
	( $code:expr ) => {
		::std::process::exit($code)
	};

	( $code :expr, $fmt:expr $( , $arg:expr )* ) => {{
        eprintln!($fmt $( , $arg )*);
		::std::process::exit($code)
	}};
}
