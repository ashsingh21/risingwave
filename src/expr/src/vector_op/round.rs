// Copyright 2023 RisingWave Labs
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use risingwave_common::types::{Decimal, F64};
use risingwave_expr_macro::function;

#[function("round_digit(decimal, int32) -> decimal")]
pub fn round_digits<D: Into<i32>>(input: Decimal, digits: D) -> Decimal {
    let digits = digits.into();
    if digits < 0 {
        Decimal::zero()
    } else {
        // rust_decimal can only handle up to 28 digits of scale
        input.round_dp_ties_away(std::cmp::min(digits as u32, 28))
    }
}

#[function("ceil(float64) -> float64")]
pub fn ceil_f64(input: F64) -> F64 {
    f64::ceil(input.0).into()
}

#[function("ceil(decimal) -> decimal")]
pub fn ceil_decimal(input: Decimal) -> Decimal {
    input.ceil()
}

#[function("floor(float64) -> float64")]
pub fn floor_f64(input: F64) -> F64 {
    f64::floor(input.0).into()
}

#[function("floor(decimal) -> decimal")]
pub fn floor_decimal(input: Decimal) -> Decimal {
    input.floor()
}

#[function("trunc(float64) -> float64")]
pub fn trunc_f64(input: F64) -> F64 {
    f64::trunc(input.0).into()
}

#[function("trunc(decimal) -> decimal")]
pub fn trunc_decimal(input: Decimal) -> Decimal {
    input.trunc()
}

// Ties are broken by rounding away from zero
#[function("round(float64) -> float64")]
pub fn round_f64(input: F64) -> F64 {
    f64::round_ties_even(input.0).into()
}

// Ties are broken by rounding away from zero
#[function("round(decimal) -> decimal")]
pub fn round_decimal(input: Decimal) -> Decimal {
    input.round_dp_ties_away(0)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use risingwave_common::types::{Decimal, F64};

    use super::ceil_f64;
    use crate::vector_op::round::*;

    fn do_test(input: &str, digits: i32, expected_output: &str) {
        let v = Decimal::from_str(input).unwrap();
        let rounded_value = round_digits(v, digits);
        assert_eq!(expected_output, rounded_value.to_string().as_str());
    }

    #[test]
    fn test_round_digits() {
        do_test("21.666666666666666666666666667", 4, "21.6667");
        do_test("84818.33333333333333333333333", 4, "84818.3333");
        do_test("84818.15", 1, "84818.2");
        do_test("21.372736", -1, "0");
        // When digit extends past original scale, it should just return original scale.
        // Intuitively, it does not make sense after rounding `0` it becomes `0.000`. Precision
        // should always be less or equal, not more.
        do_test("0", 340, "0");
    }

    #[test]
    fn test_round_f64() {
        assert_eq!(ceil_f64(F64::from(42.2)), F64::from(43.0));
        assert_eq!(ceil_f64(F64::from(-42.8)), F64::from(-42.0));

        assert_eq!(floor_f64(F64::from(42.8)), F64::from(42.0));
        assert_eq!(floor_f64(F64::from(-42.8)), F64::from(-43.0));

        assert_eq!(round_f64(F64::from(42.4)), F64::from(42.0));
        assert_eq!(round_f64(F64::from(42.5)), F64::from(42.0));
        assert_eq!(round_f64(F64::from(-6.5)), F64::from(-6.0));
        assert_eq!(round_f64(F64::from(43.5)), F64::from(44.0));
        assert_eq!(round_f64(F64::from(-7.5)), F64::from(-8.0));
    }

    #[test]
    fn test_round_decimal() {
        assert_eq!(ceil_decimal(dec(42.2)), dec(43.0));
        assert_eq!(ceil_decimal(dec(-42.8)), dec(-42.0));

        assert_eq!(floor_decimal(dec(42.2)), dec(42.0));
        assert_eq!(floor_decimal(dec(-42.8)), dec(-43.0));

        assert_eq!(round_decimal(dec(42.4)), dec(42.0));
        assert_eq!(round_decimal(dec(42.5)), dec(43.0));
        assert_eq!(round_decimal(dec(-6.5)), dec(-7.0));
    }

    fn dec(f: f64) -> Decimal {
        Decimal::try_from(f).unwrap()
    }
}
