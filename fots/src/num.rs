use num_traits::Num;

pub fn parse<T: Num>(input: &str) -> Result<T, T::FromStrRadixErr> {
    let input = input.trim();
    if input.starts_with("0x") | input.starts_with("0X") {
        return T::from_str_radix(&input[2..], 16);
    }

    if input.starts_with("0b") || input.starts_with("0B") {
        return T::from_str_radix(&input[2..], 2);
    }
    //    if input.starts_with("0") {
    //        return T::from_str_radix(input.trim_start_matches("0"), 8);
    //    }

    T::from_str_radix(input, 10)
}
