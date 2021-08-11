// Allocators
#[cfg(all(unix, target_arch = "x86_64", feature = "jemalloc"))]
mod alloc {
    pub type Allocator = tikv_jemallocator::Jemalloc;

    pub const fn allocator() -> Allocator {
        tikv_jemallocator::Jemalloc
    }
}
#[cfg(all(unix, feature = "tcmalloc"))]
mod alloc {
    pub type Allocator = tcmalloc::TCMalloc;

    pub const fn allocator() -> Allocator {
        tcmalloc::TCMalloc
    }
}
#[cfg(all(unix, feature = "mimalloc"))]
mod alloc {
    pub type Allocator = mimalloc::MiMalloc;

    pub const fn allocator() -> Allocator {
        mimalloc::MiMalloc
    }
}
#[cfg(all(unix, feature = "snmalloc"))]
mod alloc {
    pub type Allocator = snmalloc_rs::SnMalloc;

    pub const fn allocator() -> Allocator {
        snmalloc_rs::SnMalloc
    }
}
#[cfg(not(all(
    unix,
    any(
        all(feature = "jemalloc", target_arch = "x86_64"),
        feature = "tcmalloc",
        feature = "mimalloc",
        feature = "snmalloc"
    )
)))]
mod alloc {
    pub type Allocator = std::alloc::System;
    pub const fn allocator() -> Allocator {
        std::alloc::System
    }
}

#[global_allocator]
static ALLOC: alloc::Allocator = alloc::allocator();
