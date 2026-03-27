use std::fmt;
use std::ops::{Add, Mul, Neg, Sub};

use serde::{Deserialize, Serialize};

macro_rules! unit_type {
    ($name:ident, $suffix:expr, $inner:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
        pub struct $name(pub $inner);

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl Mul<$inner> for $name {
            type Output = Self;
            fn mul(self, rhs: $inner) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl Neg for $name {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{} {}", self.0, $suffix)
            }
        }

        impl $name {
            pub const ZERO: Self = Self(0.0);
        }
    };
}

// f64 units (world-scale precision)
unit_type!(Meters, "m", f64);
unit_type!(Seconds, "s", f64);
unit_type!(MetersPerSecond, "m/s", f64);

// f32 units (local-scale / subsystem values)
unit_type!(Watts, "W", f32);
unit_type!(Kilograms, "kg", f32);
unit_type!(Newtons, "N", f32);
unit_type!(Kelvin, "K", f32);
unit_type!(Liters, "L", f32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meters_arithmetic() {
        let a = Meters(10.0);
        let b = Meters(3.0);
        assert_eq!((a + b).0, 13.0);
    }

    #[test]
    fn meters_display() {
        assert_eq!(format!("{}", Meters(42.5)), "42.5 m");
    }

    #[test]
    fn seconds_arithmetic() {
        let a = Seconds(1.0);
        let b = Seconds(0.016);
        assert!((a + b).0 - 1.016 < 1e-10);
    }

    #[test]
    fn watts_arithmetic() {
        assert_eq!((Watts(100.0) + Watts(50.0)).0, 150.0);
    }

    #[test]
    fn kilograms_arithmetic() {
        assert_eq!((Kilograms(1000.0) - Kilograms(250.0)).0, 750.0);
    }

    #[test]
    fn meters_per_second_arithmetic() {
        assert_eq!((MetersPerSecond(100.0) + MetersPerSecond(30.0)).0, 130.0);
    }

    #[test]
    fn newtons_arithmetic() {
        assert_eq!((Newtons(500.0) + Newtons(200.0)).0, 700.0);
    }
}
