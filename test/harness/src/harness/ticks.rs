/// A type representing a number of ticks
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ticks(pub usize);

/// Trait for converting values to `Ticks`
pub trait ToTicks {
    fn ticks(self) -> Ticks;
}

// Unsigned integer implementations
impl ToTicks for u8 {
    fn ticks(self) -> Ticks {
        Ticks(self as usize)
    }
}

impl ToTicks for u16 {
    fn ticks(self) -> Ticks {
        Ticks(self as usize)
    }
}

impl ToTicks for u32 {
    fn ticks(self) -> Ticks {
        Ticks(self as usize)
    }
}

impl ToTicks for u64 {
    fn ticks(self) -> Ticks {
        Ticks(self as usize)
    }
}

impl ToTicks for usize {
    fn ticks(self) -> Ticks {
        Ticks(self)
    }
}

// Signed integer implementations (for integer literals which default to i32)
impl ToTicks for i8 {
    fn ticks(self) -> Ticks {
        Ticks(self.max(0) as usize)
    }
}

impl ToTicks for i16 {
    fn ticks(self) -> Ticks {
        Ticks(self.max(0) as usize)
    }
}

impl ToTicks for i32 {
    fn ticks(self) -> Ticks {
        Ticks(self.max(0) as usize)
    }
}

impl ToTicks for i64 {
    fn ticks(self) -> Ticks {
        Ticks(self.max(0) as usize)
    }
}

impl ToTicks for isize {
    fn ticks(self) -> Ticks {
        Ticks(self.max(0) as usize)
    }
}
