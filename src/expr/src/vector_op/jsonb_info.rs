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

use std::fmt::Write;

use risingwave_common::types::JsonbRef;
use risingwave_expr_macro::function;

use crate::{ExprError, Result};

#[function("jsonb_typeof(jsonb) -> varchar")]
pub fn jsonb_typeof(v: JsonbRef<'_>, writer: &mut dyn Write) {
    writer.write_str(v.type_name()).unwrap()
}

#[function("jsonb_array_length(jsonb) -> int32")]
pub fn jsonb_array_length(v: JsonbRef<'_>) -> Result<i32> {
    v.array_len()
        .map(|n| n as i32)
        .map_err(|e| ExprError::InvalidParam {
            name: "",
            reason: e,
        })
}
