use num_traits::Num;

/// Parse number value from string literal.
pub fn parse<T: Num>(input: &str) -> Result<T, T::FromStrRadixErr> {
    let mut input = input.trim();
    assert!(!input.is_empty());
    assert_ne!(&input[0..1], "+");

    let mut sign = '+';
    if input.starts_with('-') {
        input = &input[1..];
        sign = '-';
    }
    if input.starts_with("0x") | input.starts_with("0X") {
        return T::from_str_radix(&format!("{}{}", sign, &input[2..]), 16);
    }
    if input.starts_with("0b") || input.starts_with("0B") {
        return T::from_str_radix(&format!("{}{}", sign, &input[2..]), 2);
    }

    T::from_str_radix(&format!("{}{}", sign, input), 10)
}
