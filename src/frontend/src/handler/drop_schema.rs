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

use pgwire::pg_response::{PgResponse, StatementType};
use risingwave_common::catalog::is_system_schema;
use risingwave_common::error::ErrorCode::PermissionDenied;
use risingwave_common::error::{ErrorCode, Result};
use risingwave_sqlparser::ast::{DropMode, ObjectName};

use super::RwPgResponse;
use crate::binder::Binder;
use crate::catalog::CatalogError;
use crate::handler::HandlerArgs;

pub async fn handle_drop_schema(
    handler_args: HandlerArgs,
    schema_name: ObjectName,
    if_exist: bool,
    mode: Option<DropMode>,
) -> Result<RwPgResponse> {
    let session = handler_args.session;
    let catalog_reader = session.env().catalog_reader();
    let schema_name = Binder::resolve_schema_name(schema_name)?;

    if is_system_schema(&schema_name) {
        return Err(ErrorCode::ProtocolError(format!(
            "cannot drop schema {} because it is required by the database system",
            schema_name
        ))
        .into());
    }

    let schema = {
        let reader = catalog_reader.read_guard();
        match reader.get_schema_by_name(session.database(), &schema_name) {
            Ok(schema) => schema.clone(),
            Err(err) => {
                // If `if_exist` is true, not return error.
                return if if_exist {
                    Ok(PgResponse::builder(StatementType::DROP_SCHEMA)
                        .notice(format!(
                            "schema \"{}\" does not exist, skipping",
                            schema_name
                        ))
                        .into())
                } else {
                    Err(err.into())
                };
            }
        }
    };
    match mode {
        Some(DropMode::Restrict) | None => {
            if let Some(table) = schema.iter_table().next() {
                return Err(CatalogError::NotEmpty(
                    "schema",
                    schema_name,
                    "table",
                    table.name.clone(),
                )
                .into());
            }
            if let Some(source) = schema.iter_source().next() {
                return Err(CatalogError::NotEmpty(
                    "schema",
                    schema_name,
                    "source",
                    source.name.clone(),
                )
                .into());
            }
        }
        Some(DropMode::Cascade) => {
            return Err(ErrorCode::NotImplemented(
                "drop schema with cascade mode".to_string(),
                6773.into(),
            )
            .into())
        }
    };

    if session.user_id() != schema.owner() {
        return Err(PermissionDenied("Do not have the privilege".to_string()).into());
    }

    let catalog_writer = session.env().catalog_writer();
    catalog_writer.drop_schema(schema.id()).await?;
    Ok(PgResponse::empty_result(StatementType::DROP_SCHEMA))
}

#[cfg(test)]
mod tests {
    use crate::test_utils::LocalFrontend;

    #[tokio::test]
    async fn test_drop_schema() {
        let frontend = LocalFrontend::new(Default::default()).await;
        let session = frontend.session_ref();
        let catalog_reader = session.env().catalog_reader();

        frontend.run_sql("CREATE SCHEMA schema").await.unwrap();

        frontend.run_sql("CREATE TABLE schema.table").await.unwrap();

        assert!(frontend.run_sql("DROP SCHEMA schema").await.is_err());

        frontend.run_sql("DROP TABLE schema.table").await.unwrap();

        frontend.run_sql("DROP SCHEMA schema").await.unwrap();

        let schema = catalog_reader
            .read_guard()
            .get_database_by_name("schema")
            .ok()
            .cloned();
        assert!(schema.is_none());
    }
}
