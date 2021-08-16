use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
};

use super::TypeId;

#[derive(Debug, Clone)]
pub struct CommonInfo {
    id: TypeId, // must be unique
    name: Box<str>,
    size: u64,
    align: u64,
    optional: bool,
    varlen: bool,
}

impl CommonInfo {
    #[inline(always)]
    pub fn id(&self) -> TypeId {
        self.id
    }

    #[inline(always)]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline(always)]
    pub fn size(&self) -> u64 {
        self.size
    }

    #[inline(always)]
    pub fn align(&self) -> u64 {
        self.align
    }

    #[inline(always)]
    pub fn optional(&self) -> bool {
        self.optional
    }

    #[inline(always)]
    pub fn varlen(&self) -> bool {
        self.varlen
    }

    pub fn template_name(&self) -> String {
        if let Some(idx) = self.name.find('[') {
            self.name[..idx].to_string()
        } else {
            String::from(&self.name[..])
        }
    }
}

impl PartialEq for CommonInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for CommonInfo {}

impl PartialOrd for CommonInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl Ord for CommonInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl Hash for CommonInfo {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.id.hash(hasher)
    }
}

macro_rules! common_attr_getter {
    () => {
        #[inline(always)]
        pub fn id(&self) -> crate::ty::TypeId {
            self.comm.id()
        }

        #[inline(always)]
        pub fn name(&self) -> &str {
            &self.comm.name()
        }

        #[inline(always)]
        pub fn size(&self) -> u64 {
            self.comm.size()
        }

        #[inline(always)]
        pub fn align(&self) -> u64 {
            self.comm.align()
        }

        #[inline(always)]
        pub fn optional(&self) -> bool {
            self.comm.optional()
        }

        #[inline(always)]
        pub fn varlen(&self) -> bool {
            self.comm.varlen()
        }

        #[inline(always)]
        pub fn template_name(&self) -> String {
            self.comm.template_name()
        }

        #[inline(always)]
        pub fn comm(&self) -> &crate::ty::common::CommonInfo {
            &self.comm
        }
    };
}

macro_rules! default_int_format_attr_getter {
    () => {
        #[inline(always)]
        pub fn bitfield_off(&self) -> u64 {
            0
        }

        #[inline(always)]
        pub fn bitfield_len(&self) -> u64 {
            0
        }

        #[inline(always)]
        pub fn bitfield_unit(&self) -> u64 {
            self.size()
        }

        #[inline(always)]
        pub fn bitfield_unit_off(&self) -> u64 {
            0
        }

        #[inline(always)]
        pub fn is_bitfield(&self) -> bool {
            false
        }
    };
}

macro_rules! extra_attr_getter {
    () => {
        #[inline(always)]
        pub fn bin_fmt(&self) -> crate::ty::BinaryFormat {
            crate::ty::BinaryFormat::Native
        }
    };
}

macro_rules! eq_ord_hash_impl {
    ($impl_ty: ty) => {
        impl PartialEq for $impl_ty {
            fn eq(&self, other: &Self) -> bool {
                self.id() == other.id()
            }
        }

        impl Eq for $impl_ty {}

        impl PartialOrd for $impl_ty {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                self.id().partial_cmp(&other.id())
            }
        }

        impl Ord for $impl_ty {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.id().cmp(&other.id())
            }
        }

        impl std::hash::Hash for $impl_ty {
            fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
                self.id().hash(hasher)
            }
        }
    };
}

#[derive(Debug, Clone)]
pub struct CommonInfoBuilder {
    id: TypeId,
    name: String,
    size: Option<u64>,
    align: Option<u64>,
    optional: bool,
    varlen: bool,
}

impl Default for CommonInfoBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CommonInfoBuilder {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            id: TypeId::MAX,
            size: None,
            align: None,
            optional: false,
            varlen: false,
        }
    }

    pub fn id(&mut self, id: TypeId) -> &mut Self {
        self.id = id;
        self
    }

    pub fn name<T: Into<String>>(&mut self, name: T) -> &mut Self {
        self.name = name.into();
        self
    }

    pub fn size(&mut self, size: u64) -> &mut Self {
        self.size = Some(size);
        self
    }

    pub fn align(&mut self, align: u64) -> &mut Self {
        self.align = Some(align);
        self
    }

    pub fn optional(&mut self, optional: bool) -> &mut Self {
        self.optional = optional;
        self
    }

    pub fn varlen(&mut self, varlen: bool) -> &mut Self {
        self.varlen = varlen;
        self
    }

    pub fn build(self) -> CommonInfo {
        let size = self.size.unwrap_or_default();
        let align = self.align.unwrap_or_default();
        let varlen = self.size.is_none();

        CommonInfo {
            id: self.id,
            name: self.name.into_boxed_str(),
            optional: self.optional,

            size,
            align,
            varlen,
        }
    }
}
