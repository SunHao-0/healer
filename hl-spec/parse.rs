use crate::error::Error;
use crate::util::num::Integer;
use nom::bytes::complete::{take, take_while};
use nom::character::{is_digit, is_hex_digit, is_oct_digit};
use nom::{AsChar, IResult};
use nom_locate::LocatedSpan;

/// Location of current parsing target, second parameter stands for filename
pub type Span<'a> = LocatedSpan<&'a str, String>;

/// Parse an identifier.
/// An identifier should be ([a-zA-Z]|_)(alpha|num|_)*
pub(crate) fn parse_ident(input: Span) -> IResult<Span, &str, Error> {
    let (input, fst_ch) = peek_one(input).map_err(|e| {
        if let nom::Err::Error(e) = e {
            make_nom_err(Error::add_context(e, "Identifier".into()))
        } else {
            unreachable!()
        }
    })?;
    if !fst_ch.is_alpha() && fst_ch != '_' {
        let err: String = if fst_ch.is_whitespace() {
            "Whitespace".into()
        } else if fst_ch.is_control() {
            fst_ch.escape_unicode().collect()
        } else {
            fst_ch.into()
        };
        Err(make_nom_err(Error {
            span: input,
            expect: "Identifier".into(),
            found: err,
            context: None,
        }))
    } else {
        // We know this input contains at least one legal character.
        let (new_input, ident) = take_while::<_, _, ()>(is_ident_ch)(input.clone()).unwrap();
        let ident = ident.fragment();
        if &"_" == ident {
            return Err(make_nom_err(Error {
                span: input,
                expect: "Identifier".into(),
                found: "Single \'_\'".into(),
                context: None,
            }));
        }
        Ok((new_input, ident))
    }
}

/// Parse an integer literal.
/// [-][0x|0b|0o][0-9|a-f]+
pub(crate) fn parse_integer<T: Integer>(input: Span) -> IResult<Span, T, Error> {
    let mut sign = 1;
    //    let mut base = 10;

    let (mut input, mut fst_ch) = peek_one(input)?;

    if fst_ch == '-' {
        sign = -1;
        let (new_input, _) = take::<_, _, ()>(1usize)(input).unwrap();
        let ret = peek_one(new_input)?;
        input = ret.0;
        fst_ch = ret.1;
    }

    if fst_ch == '0' {
        let (new_input, _) = take::<_, _, ()>(1usize)(input).unwrap();
        input = new_input;
        return if let Ok((input, ch)) = peek_one(input.clone()) {
            match ch {
                'x' | 'X' => {
                    let (new_input, _) = take::<_, _, ()>(1usize)(input).unwrap();
                    let (new_input, num) =
                        take_while::<_, _, ()>(|c| is_hex_digit(c as u8))(new_input.clone())
                            .map_err(|_e| {
                                nom::Err::Error(Error {
                                    span: new_input,
                                    expect: "0-9, a-f, A-F".to_string(),
                                    found: "".to_string(),
                                    context: Option::None,
                                })
                            })?;
                    Ok((new_input, T::from_str_radix(num.fragment(), 16).unwrap()))
                }
                'b' => {
                    let (new_input, _) = take::<_, _, ()>(1usize)(input).unwrap();
                    let (new_input, num) =
                        take_while::<_, _, ()>(|c| c == '0' || c == '1')(new_input.clone())
                            .map_err(|_e| {
                                nom::Err::Error(Error {
                                    span: new_input,
                                    expect: "0, 1".to_string(),
                                    found: "".to_string(),
                                    context: Option::None,
                                })
                            })?;
                    Ok((new_input, T::from_str_radix(num.fragment(), 2).unwrap()))
                }
                'o' | 'O' => {
                    let (new_input, _) = take::<_, _, ()>(1usize)(input).unwrap();
                    let (new_input, num) =
                        take_while::<_, _, ()>(|c| is_oct_digit(c as u8))(new_input.clone())
                            .map_err(|_e| {
                                nom::Err::Error(Error {
                                    span: new_input,
                                    expect: "0-7".to_string(),
                                    found: "".to_string(),
                                    context: Option::None,
                                })
                            })?;
                    Ok((new_input, T::from_str_radix(num.fragment(), 8).unwrap()))
                }
                _ => {
                    let (new_input, num) = take_while::<_, _, ()>(|c| is_digit(c as u8))(
                        input.clone(),
                    )
                    .map_err(|e| {
                        nom::Err::Error(Error {
                            span: input,
                            expect: "0-9".to_string(),
                            found: "".to_string(),
                            context: Option::None,
                        })
                    })?;
                    Ok((new_input, T::from_str_radix(num.fragment(), 10).unwrap()))
                }
            }
        } else {
            Ok((input, T::from_str_radix("0", 10).unwrap()))
        };
    }

    Err(nom::Err::Error(Error {
        span: input,
        expect: "0-9, -".to_string(),
        found: fst_ch.to_string(),
        context: Some("parse integer".to_string()),
    }))
}

fn peek_one(input: Span) -> IResult<Span, char, Error> {
    if let Some(fst_ch) = input.fragment().chars().next() {
        Ok((input, fst_ch))
    } else {
        Err(make_nom_err(Error {
            span: input,
            expect: "expected character".into(),
            found: "EOF".into(),
            context: None,
        }))
    }
}

pub(crate) fn parse_str(input: Span) -> IResult<Span, String, Error> {
    todo!()
}

fn is_ident_ch(ch: char) -> bool {
    match ch {
        'a'..='z' | 'A'..='Z' => true,
        '_' => true,
        '0'..='9' => true,
        _ => false,
    }
}

fn make_nom_err(e: Error) -> nom::Err<Error> {
    nom::Err::Error(e)
}

fn make_nom_failure(e: Error) -> nom::Err<Error> {
    nom::Err::Failure(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn new_span(span: &str) -> Span {
        Span::new_extra(span, String::new())
    }

    #[test]
    fn test_parse_ident() {
        let s2 = new_span("@");
        assert!(parse_ident(s2).is_err());
        let s1 = new_span("_");
        assert!(parse_ident(s1).is_err());

        let s3 = new_span("_test");
        assert_eq!(
            parse_ident(s3),
            Ok((
                unsafe { Span::new_from_raw_offset(5, 1, "", String::new()) },
                "_test"
            ))
        );
        let s5 = new_span("test_1123,");
        assert_eq!(
            parse_ident(s5),
            Ok((
                unsafe { Span::new_from_raw_offset(9, 1, ",", String::new()) },
                "test_1123"
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

        /*
        let s5 = "-256";
        let (_, s5_number) = parse_integer::<i16>(new_span(s5)).unwrap();
        assert_eq!(s5_number, -256); */
    }
}
