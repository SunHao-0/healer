use std::num::ParseIntError;

pub trait Integer: Sized + Copy {
    const MIN: Self;
    const MAX: Self;

    fn from_str_radix(input: &str, r: u32) -> Result<Self, ParseIntError>;
}

macro_rules! impl_integer {
    ($t:ty) => {
        impl Integer for $t {
            const MIN: $t = <$t>::MIN;
            const MAX: $t = <$t>::MAX;

            fn from_str_radix(input: &str, r: u32) -> Result<$t, ParseIntError> {
                <$t>::from_str_radix(input, r)
            }
        }
    };
}

impl_integer!(u8);
impl_integer!(u16);
impl_integer!(u32);
impl_integer!(u64);
impl_integer!(i8);
impl_integer!(i16);
impl_integer!(i32);
impl_integer!(i64);
impl_integer!(usize);
impl_integer!(isize);
