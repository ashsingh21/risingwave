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

use risingwave_common::types::DataType;

use crate::catalog::system_catalog::SystemCatalogColumnsDef;

pub const RW_INDEXES_TABLE_NAME: &str = "rw_indexes";

pub const RW_INDEXES_COLUMNS: &[SystemCatalogColumnsDef<'_>] = &[
    (DataType::Int32, "id"),
    (DataType::Varchar, "name"),
    (DataType::Int32, "schema_id"),
    (DataType::Int32, "owner"),
    (DataType::Varchar, "definition"),
    (DataType::Varchar, "acl"),
];
