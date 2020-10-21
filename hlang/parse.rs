use crate::error::ParseError;
use crate::util::num::Integer;
use nom::bytes::complete::{take, take_while, take_while1};
use nom::character::{is_digit, is_hex_digit, is_oct_digit};
use nom::{AsChar, IResult};
use nom_locate::LocatedSpan;

/// Location of current parsing target, second parameter stands for filename
pub type Span<'a> = LocatedSpan<&'a str, ()>;

/// Parse an identifier.
/// An identifier should be ([a-zA-Z]|_)(alpha|num|_)*
pub(crate) fn parse_ident(input: Span) -> IResult<Span, &str, ParseError> {
    const LEGAL_LEADING_CH: &str = "a-z, A-Z, _";

    let fst_ch = peek_one(input).map_err(|e| {
        e.add_context("identifier parsing")
            .expect(LEGAL_LEADING_CH)
            .into()
    })?;

    if !fst_ch.is_alpha() && fst_ch != '_' {
        let err = if fst_ch.is_whitespace() {
            // make white space visible
            "whitespace".to_string()
        } else if fst_ch.is_control() {
            fst_ch.escape_unicode().collect()
        } else {
            fst_ch.to_string()
        };
        Err(ParseError::new(input)
            .expect(LEGAL_LEADING_CH)
            .found(err)
            .add_context("identifier parsing")
            .into())
    } else {
        // We know this input contains at least one legal character.
        let (input, ident) = take_while::<_, _, ()>(is_ident_ch)(input).unwrap();
        Ok((input, ident.fragment()))
    }
}

/// Parse an integer literal.
/// An Integer should be [-][0x|0b|0o][0-9|a-f|A-F]+
pub(crate) fn parse_integer<T: Integer>(mut input: Span) -> IResult<Span, T, ParseError> {
    let mut sign = 1;
    let mut fst_ch =
        peek_one(input).map_err(|e| e.add_context("integer parsing").expect("-, 0-9,").into())?;
    let target_line = input.fragment().lines().next().unwrap(); // We know input must contain at least one line.

    if fst_ch == '-' {
        sign = -1;
        input = eat_one(input);
        fst_ch = peek_one(input).map_err(|e| {
            e.err_snippets(target_line)
                .add_context("integer parsing")
                .expect("0-9")
                .into()
        })?;
    }

    if fst_ch == '0' {
        input = eat_one(input);

        return if let Ok(ch) = peek_one(input) {
            match ch {
                'x' | 'X' => {
                    input = eat_one(input);
                    let (new_input, num) = take_while1::<_, _, ()>(|c| is_hex_digit(c as u8))(
                        input,
                    )
                    .map_err(|_| {
                        ParseError::new(input)
                            .err_snippets(target_line)
                            .add_context("integer parsing")
                            .expect("0-9, a-f, A-F")
                            .found("non hex digit")
                            .into()
                    })?;
                    let num = T::from_str_radix(num.fragment(), 16).map_err(|e| {
                        ParseError::new(input)
                            .err_snippets(target_line)
                            .add_context("integer parsing")
                            .expect(format!("integer in range ({}, {})", T::MIN, T::MAX))
                            .found(format!("error: {}", e))
                            .into()
                    })?;
                    Ok((new_input, T::maybe_change_sign(num, sign)))
                }
                'b' | 'B' => {
                    input = eat_one(input);
                    let (new_input, num) = take_while1::<_, _, ()>(|c| c == '0' || c == '1')(input)
                        .map_err(|_| {
                            ParseError::new(input)
                                .err_snippets(target_line)
                                .add_context("integer parsing")
                                .expect("0, 1")
                                .found("non 0/1 digit")
                                .into()
                        })?;
                    let num = T::from_str_radix(num.fragment(), 2).map_err(|e| {
                        ParseError::new(input)
                            .err_snippets(target_line)
                            .add_context("integer parsing")
                            .expect(format!("integer in range ({}, {})", T::MIN, T::MAX))
                            .found(format!("error: {}", e))
                            .into()
                    })?;
                    Ok((new_input, T::maybe_change_sign(num, sign)))
                }
                'o' | 'O' => {
                    input = eat_one(input);
                    let (new_input, num) = take_while1::<_, _, ()>(|c| is_oct_digit(c as u8))(
                        input,
                    )
                    .map_err(|_| {
                        ParseError::new(input)
                            .err_snippets(target_line)
                            .add_context("integer parsing")
                            .expect("0-7")
                            .found("non 0-7 dight")
                            .into()
                    })?;
                    let num = T::from_str_radix(num.fragment(), 8).map_err(|e| {
                        ParseError::new(input)
                            .err_snippets(target_line)
                            .add_context("integer parsing")
                            .expect(format!("integer in range ({}, {})", T::MIN, T::MAX))
                            .found(format!("error: {}", e))
                            .into()
                    })?;
                    Ok((new_input, T::maybe_change_sign(num, sign)))
                }
                '0'..='9' => {
                    // We already got one digit
                    let (new_input, num) =
                        take_while::<_, _, ()>(|c| is_digit(c as u8))(input).unwrap();
                    let num = T::from_str_radix(num.fragment(), 10).map_err(|e| {
                        ParseError::new(input)
                            .err_snippets(target_line)
                            .add_context("integre parsing")
                            .expect(format!("integer in range: ({}, {})", T::MIN, T::MAX))
                            .found(format!("error: {}", e))
                            .into()
                    })?;
                    Ok((new_input, T::maybe_change_sign(num, sign)))
                }
                _ => Ok((input, T::zero())),
            }
        } else {
            Ok((input, T::zero()))
        };
    } // fst_ch == '0'

    let (new_input, num) = take_while1::<_, _, ()>(|c| is_digit(c as _))(input).map_err(|_| {
        ParseError::new(input)
            .err_snippets(target_line)
            .add_context("integer parsing")
            .expect("0-9")
            .found("non digit value")
            .into()
    })?;
    let num = T::from_str_radix(num.fragment(), 10).map_err(|e| {
        ParseError::new(input)
            .err_snippets(target_line)
            .add_context("integer parsing")
            .expect(format!("integer in range: ({}, {})", T::MIN, T::MAX))
            .found(format!("error: {}", e))
            .into()
    })?;
    Ok((new_input, T::maybe_change_sign(num, sign)))
}

/// Read one character without consume any byte.
fn peek_one(input: Span) -> Result<char, ParseError> {
    if let Some(fst_ch) = input.fragment().chars().next() {
        Ok(fst_ch)
    } else {
        Err(ParseError::new(input).found("EOF").err_snippets("EOF"))
    }
}

fn eat_one(input: Span) -> Span {
    let (out, _) = take::<_, _, ()>(1usize)(input).unwrap();
    out
}

fn is_ident_ch(ch: char) -> bool {
    match ch {
        'a'..='z' | 'A'..='Z' => true,
        '_' => true,
        '0'..='9' => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn new_span(span: &str) -> Span {
        Span::new(span)
    }

    #[test]
    fn test_parse_ident() {
        assert!(parse_ident(Span::new("")).is_err());

        let s2 = new_span("@");
        assert!(parse_ident(s2).is_err());

        let s1 = new_span("_");
        assert!(parse_ident(s1).is_ok());

        let s3 = new_span("_t_1@@@");
        assert_eq!(
            parse_ident(s3),
            Ok((
                unsafe { Span::new_from_raw_offset(4, 1, "@@@", ()) },
                "_t_1"
            ))
        );
    }

    #[test]
    fn test_parse_integer() {
        let s1 = "test";
        assert!(parse_integer::<i32>(new_span(s1)).is_err());

        let s2 = new_span("-0");
        let (_, s2_number) = parse_integer::<i8>(s2).unwrap();
        assert_eq!(s2_number, 0);

        let s2 = "0xFFF";
        let s3 = "0xfff";
        assert_eq!(
            parse_integer::<u32>(new_span(s2)),
            parse_integer::<u32>(new_span(s3))
        );

        assert_eq!(
            parse_integer::<u32>(new_span("0b001")).unwrap().1,
            parse_integer::<u32>(new_span("0b00001")).unwrap().1
        );

        let s4 = "099";
        let (_, s4_number) = parse_integer::<u8>(new_span(s4)).unwrap();
        assert_eq!(s4_number, 99);

        let s5 = "-256";
        let (_, s5_number) = parse_integer::<i16>(new_span(s5)).unwrap();
        assert_eq!(s5_number, -256);
    }
}
