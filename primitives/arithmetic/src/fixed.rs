// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

#[cfg(feature = "std")]
use serde::{Serialize, Deserialize};

use sp_std::{ops, fmt, prelude::*, convert::TryInto};
use codec::{Encode, CompactAs};
use crate::traits::{
	SaturatedConversion, UniqueSaturatedInto, Saturating, BaseArithmetic,
	Bounded, Zero, FixedPointNumber
};
use sp_debug_derive::RuntimeDebug;

#[macro_export]
macro_rules! implement_fixed {
	(
		$name:ident,
		$inner_type:ty,
		$unsigned_type:ty,
		$prev_unsigned_type:ty,
		$perthing_type:ty,
		$div:tt,
		$precision:tt,
	) => {
		#[derive(Encode, Decode, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
		pub struct $name($inner_type);

		impl From<$inner_type> for $name {
			fn from(n: $inner_type) -> Self {
				Self(n.saturating_mul(Self::DIV))
			}
		}

		impl FixedPointNumber for $name {
			type Inner = $inner_type;
			type Unsigned = $unsigned_type;
			type PrevUnsigned = $prev_unsigned_type;
			type Perthing = $perthing_type;

			const DIV: $inner_type = $div;

			fn from_inner(inner: Self::Inner) -> Self {
				Self(inner)
			}
		
			fn from_integer(int: Self::Inner) -> Self {
				Self(int.saturating_mul(Self::DIV))
			}
		
			fn checked_from_integer(int: Self::Inner) -> Option<Self> {
				int.checked_mul(Self::DIV).map(|inner| Self(inner))
			}
		
			fn from_rational<N: UniqueSaturatedInto<Self::Inner>>(n: N, d: Self::Inner) -> Self {
				Self((n.unique_saturated_into() * Self::DIV) / d)
			}
		
			fn checked_from_rational<N: TryInto<Self::Inner>>(n: N, d: Self::Inner) -> Option<Self> {
				if d == 0 {
					return None
				}
		
				n.try_into().ok()
					.and_then(|n| n.checked_mul(Self::DIV))
					.and_then(|n| n.checked_div(d))
					.and_then(|n| Some(Self(n)))
			}
		
			fn into_inner(self) -> Self::Inner {
				self.0
			}
		
			fn checked_mul_int<N>(&self, other: &N) -> Option<N>
			where
				N: Copy + TryFrom<Self::Inner> + TryInto<Self::Inner>,
			{
				N::try_into(*other).ok().and_then(|rhs| {
					let mut lhs = self.0;
					if lhs.is_negative() {
						lhs = lhs.saturating_mul(-1);
					}
					let mut rhs: Self::Inner = rhs.saturated_into();
					let signum = self.0.signum() * rhs.signum();
					if rhs.is_negative() {
						rhs = rhs.saturating_mul(-1);
					}
		
					(lhs as Self::Unsigned)
						.checked_mul(rhs as Self::Unsigned)
						.and_then(|n| n.checked_div(Self::DIV as Self::Unsigned))
						.and_then(|n| TryInto::<Self::Inner>::try_into(n).ok())
						.and_then(|n| TryInto::<N>::try_into(n * signum).ok())
				})
			}
		
			fn checked_div_int<N>(&self, other: &N) -> Option<N>
			where
				N: Copy + TryFrom<Self::Inner> + TryInto<Self::Inner>,
			{
				N::try_into(*other)
					.ok()
					.and_then(|n| self.0.checked_div(n))
					.and_then(|n| n.checked_div(Self::DIV))
					.and_then(|n| TryInto::<N>::try_into(n).ok())
			}
		
			fn saturating_mul_int<N>(&self, other: &N) -> N
			where
				N: Copy + TryFrom<Self::Inner> + TryInto<Self::Inner> + Bounded + Signed,
			{
				self.checked_mul_int(other).unwrap_or_else(|| {
					let signum = other.signum().saturated_into() * self.0.signum();
					if signum.is_negative() {
						Bounded::min_value()
					} else {
						Bounded::max_value()
					}
				})
			}
		
			fn saturating_abs(&self) -> Self {
				if self.0 == Self::Inner::min_value() {
					return Self::max_value();
				}
		
				if self.0.is_negative() {
					Self::from_inner(self.0 * -1)
				} else {
					*self
				}
			}
		
			fn zero() -> Self {
				Self(0)
			}
		
			fn is_zero(&self) -> bool {
				self.0 == 0
			}
		
			fn one() -> Self {
				Self(Self::DIV)
			}
		
			fn is_positive(&self) -> bool {
				self.0.is_positive()
			}
		
			fn is_negative(&self) -> bool {
				self.0.is_negative()
			}
		
			fn saturated_multiply_accumulate<N>(self, int: N) -> N
				where
					N: From<Self::PrevUnsigned> + TryFrom<Self::Unsigned> + UniqueSaturatedInto<Self::PrevUnsigned> +
					Bounded + Copy + Saturating +
					ops::Rem<N, Output=N> + ops::Div<N, Output=N> + ops::Mul<N, Output=N> +
					ops::Add<N, Output=N> + Default + core::fmt::Debug
			{
				let div = Self::DIV as Self::Unsigned;
				let positive = self.0 > 0;
				// safe to convert as absolute value.
				let parts  = self.0.checked_abs().map(|v| v as Self::Unsigned)
					.unwrap_or(Self::Inner::max_value() as Self::Unsigned + 1);
		
				let natural_parts = parts / div;
				let natural_parts: N = natural_parts.saturated_into();
		
				let fractional_parts = (parts % div) as Self::PrevUnsigned;
		
				let n = int.saturating_mul(natural_parts);
				let p = Self::Perthing::from_parts(fractional_parts) * int;
		
				// everything that needs to be either added or subtracted from the original `int`.
				let excess = n.saturating_add(p);
		
				if positive {
					int.saturating_add(excess)
				} else {
					int.saturating_sub(excess)
				}
			}
		}

		impl Saturating for $name {
			fn saturating_add(self, rhs: Self) -> Self {
				Self(self.0.saturating_add(rhs.0))
			}

			fn saturating_mul(self, rhs: Self) -> Self {
				self.checked_mul(&rhs).unwrap_or_else(|| {
					if (self.0.signum() * rhs.0.signum()).is_negative() {
						Bounded::min_value()
					} else {
						Bounded::max_value()
					}
				})
			}

			fn saturating_sub(self, rhs: Self) -> Self {
				Self(self.0.saturating_sub(rhs.0))
			}

			fn saturating_pow(self, exp: usize) -> Self {
				if exp == 0 {
					return Self::from_integer(1);
				}

				let exp = exp as u64;
				let msb_pos = 64 - exp.leading_zeros();

				let mut result = Self::from_integer(1);
				let mut pow_val = self;
				for i in 0..msb_pos {
					if ((1 << i) & exp) > 0 {
						result = result.saturating_mul(pow_val);
					}
					pow_val = pow_val.saturating_mul(pow_val);
				}
				result
			}
		}


		impl ops::Add for $name {
			type Output = Self;

			fn add(self, rhs: Self) -> Self::Output {
				Self(self.0 + rhs.0)
			}
		}

		impl ops::Sub for $name {
			type Output = Self;

			fn sub(self, rhs: Self) -> Self::Output {
				Self(self.0 - rhs.0)
			}
		}

		impl ops::Mul for $name {
			type Output = Self;

			fn mul(self, rhs: Self) -> Self::Output {
				Self((self.0 * rhs.0) / Self::DIV)
			}
		}

		impl ops::Div for $name {
			type Output = Self;

			fn div(self, rhs: Self) -> Self::Output {
				Self((self.0 * Self::DIV) / rhs.0)
			}
		}

		impl CheckedSub for $name {
			fn checked_sub(&self, rhs: &Self) -> Option<Self> {
				self.0.checked_sub(rhs.0).map(Self)
			}
		}

		impl CheckedAdd for $name {
			fn checked_add(&self, rhs: &Self) -> Option<Self> {
				self.0.checked_add(rhs.0).map(Self)
			}
		}

		impl CheckedDiv for $name {
			fn checked_div(&self, rhs: &Self) -> Option<Self> {
				if rhs.0.signum() == 0 || (*self == Self::min_value() && *rhs == Self::from_integer(-1)){
					return None;
				}

				if self.0 == 0 {
					return Some(*self);
				}

				let signum = self.0.signum() / rhs.0.signum();
				let mut lhs = self.0;
				if lhs.is_negative() {
					lhs = lhs.saturating_mul(-1);
				}

				let mut rhs = rhs.0;
				if rhs.is_negative() {
					rhs = rhs.saturating_mul(-1);
				}

				multiply_by_rational(lhs as u128, <Self as FixedPointNumber>::DIV as u128, rhs as u128)
					.ok()
					.and_then(|n| TryInto::<<Self as FixedPointNumber>::Inner>::try_into(n).ok())
					.map(|n| Self(n * signum))
			}
		}

		impl CheckedMul for $name {
			fn checked_mul(&self, rhs: &Self) -> Option<Self> {
				let signum = self.0.signum() * rhs.0.signum();
				let mut lhs = self.0;

				if lhs.is_negative() {
					lhs = lhs.saturating_mul(-1);
				}
				let mut rhs = rhs.0;
				if rhs.is_negative() {
					rhs = rhs.saturating_mul(-1);
				}

				multiply_by_rational(lhs as u128, rhs as u128, <Self as FixedPointNumber>::DIV as u128)
					.ok()
					.and_then(|n| TryInto::<<Self as FixedPointNumber>::Inner>::try_into(n).ok())
					.map(|n| Self(n * signum))
			}
		}

		impl Bounded for $name {
			fn min_value() -> Self {
				Self(Bounded::min_value())
			}

			fn max_value() -> Self {
				Self(Bounded::max_value())
			}
		}

		impl sp_std::fmt::Debug for $name {
			#[cfg(feature = "std")]
			fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
				let integral = {
					let int = self.0 / Self::DIV;
					let signum_for_zero = if int == 0 && self.is_negative() { "-" } else { "" };
					format!("{}{}", signum_for_zero, int)
				};
				let fractional = format!("{:0>width$}", (self.0 % Self::DIV).abs(), width=$precision);
				write!(f, "{}({}.{})", stringify!($name), integral, fractional)
			}

			#[cfg(not(feature = "std"))]
			fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
				Ok(())
			}
		}

		impl<P: PerThing> From<P> for $name {
			fn from(p: P) -> Self {
				let accuracy = P::ACCURACY.saturated_into() as <Self as FixedPointNumber>::Inner;
				let value = p.deconstruct().saturated_into() as <Self as FixedPointNumber>::Inner;
				$name::checked_from_rational(value, accuracy).unwrap_or($name::max_value())
			}
		}

		#[cfg(feature = "std")]
		impl $name {
			fn get_str(&self) -> String {
				format!("{}", &self.0)
			}

			fn try_from_str(s: &str) -> Result<Self, &'static str> {
				let inner: <Self as FixedPointNumber>::Inner = s.parse().map_err(|_| "invalid string input")?;
				Ok(Self::from_inner(inner))
			}
		}

		// Manual impl `Serialize` as serde_json does not support i128.
		// TODO: remove impl if issue https://github.com/serde-rs/json/issues/548 fixed.
		#[cfg(feature = "std")]
		impl Serialize for $name {
			fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
			where
				S: Serializer,
			{
				serializer.serialize_str(&self.get_str())
			}
		}

		// Manual impl `Serialize` as serde_json does not support i128.
		// TODO: remove impl if issue https://github.com/serde-rs/json/issues/548 fixed.
		#[cfg(feature = "std")]
		impl<'de> Deserialize<'de> for $name {
			fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
			where
				D: Deserializer<'de>,
			{
				let s = String::deserialize(deserializer)?;
				$name::try_from_str(&s).map_err(|err_str| de::Error::custom(err_str))
			}
		}

		#[cfg(test)]
		mod tests {
			use super::*;
			use crate::{Perbill, Percent, Permill, Perquintill};

			fn max() -> $name {
				$name::max_value()
			}

			fn min() -> $name {
				$name::min_value()
			}

			#[test]
			fn from_integer_works() {
				let inner_max = <$name as FixedPointNumber>::Inner::max_value();
				let inner_min = <$name as FixedPointNumber>::Inner::min_value();
				let accuracy = $name::accuracy();

				// Cases where integer fits.
				let a = $name::from_integer(42);
				assert_eq!(a.into_inner(), 42 * accuracy);

				let a = $name::from_integer(-42);
				assert_eq!(a.into_inner(), -42 * accuracy);

				// Max/min integers that fit.
				let a = $name::from_integer(inner_max / accuracy);
				assert_eq!(a.into_inner(), (inner_max / accuracy) * accuracy);

				let a = $name::from_integer(inner_min / accuracy);
				assert_eq!(a.into_inner(), (inner_min / accuracy) * accuracy);

				// Cases where integer doesn't fit, so it saturates.
				let a = $name::from_integer(inner_max / accuracy + 1);
				assert_eq!(a.into_inner(), inner_max);

				let a = $name::from_integer(inner_min / accuracy - 1);
				assert_eq!(a.into_inner(), inner_min);
			}

			#[test]
			fn checked_from_integer_works() {
				let inner_max = <$name as FixedPointNumber>::Inner::max_value();
				let inner_min = <$name as FixedPointNumber>::Inner::min_value();
				let accuracy = $name::accuracy();

				// Cases where integer fits.
				let a = $name::checked_from_integer(42)
					.expect("42 * accuracy <= inner_max; qed");
				assert_eq!(a.into_inner(), 42 * accuracy);

				let a = $name::checked_from_integer(-42)
					.expect("-42 * accuracy >= inner_min; qed");
				assert_eq!(a.into_inner(), -42 * accuracy);

				// Max/min integers that fit.
				let a = $name::checked_from_integer(inner_max / accuracy)
					.expect("(inner_max / accuracy) * accuracy <= inner_max; qed");
				assert_eq!(a.into_inner(), (inner_max / accuracy) * accuracy);

				let a = $name::checked_from_integer(inner_min / accuracy)
					.expect("(inner_min / accuracy) * accuracy <= inner_min; qed");
				assert_eq!(a.into_inner(), (inner_min / accuracy) * accuracy);

				// Cases where integer doesn't fit, so it returns `None`.
				let a = $name::checked_from_integer(inner_max / accuracy + 1);
				assert_eq!(a, None);

				let a = $name::checked_from_integer(inner_min / accuracy - 1);
				assert_eq!(a, None);
			}

			#[test]
			fn from_inner_works() {
				let inner_max = <$name as FixedPointNumber>::Inner::max_value();
				let inner_min = <$name as FixedPointNumber>::Inner::min_value();

				assert_eq!(max(), $name::from_inner(inner_max));
				assert_eq!(min(), $name::from_inner(inner_min));
			}

			#[test]
			fn from_rational_works() {
				let a = $name::from_rational(5, 2);
				let b = $name::from_rational(-5, 2);
				let c = $name::from_rational(5, -2);
				let d = $name::from_rational(-5, -2);

				assert_eq!(a.saturating_mul_int(10), 25.into());
				assert_eq!(b.saturating_mul_int(10),-25.into());
				assert_eq!(c.saturating_mul_int(10), -25.into());
				assert_eq!(d.saturating_mul_int(10), 25.into());
			}

			#[test]
			fn mul_works() {
				let a = $name::from_integer(1);
				let b = $name::from_integer(2);
				let c = a * b;
				assert_eq!(c, b);
			}
		}
	}
}