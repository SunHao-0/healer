pub struct Split {
    base: usize,
    n: usize,
    left: usize,
}

impl Split {
    pub fn new(total: usize, n: usize) -> Split {
        assert!(total >= n);
        let base = total / n;
        let left = total % n;
        Self { base, n, left }
    }
}

impl Iterator for Split {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            if self.left != 0 {
                self.left -= 1;
                Some(self.base + 1)
            } else {
                Some(self.base)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::split::Split;
    use supper::*;

    #[test]
    fn split() {
        let s1 = Split::new(9, 9);
        let s2 = Split::new(10, 9);
        let s3 = Split::new(19, 3);

        assert_eq!(s1.sum(), 9);
        assert_eq!(s2.sum(), 10);
        assert_eq!(s3.sum(), 19);
    }
}
