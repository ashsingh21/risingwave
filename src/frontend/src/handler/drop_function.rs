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

use pgwire::pg_response::StatementType;
use risingwave_sqlparser::ast::{DropFunctionDesc, ReferentialAction};

use super::*;
use crate::catalog::root_catalog::SchemaPath;
use crate::catalog::CatalogError;
use crate::{bind_data_type, Binder};

pub async fn handle_drop_function(
    handler_args: HandlerArgs,
    if_exists: bool,
    mut func_desc: Vec<DropFunctionDesc>,
    _option: Option<ReferentialAction>,
) -> Result<RwPgResponse> {
    if func_desc.len() != 1 {
        return Err(ErrorCode::NotImplemented(
            "only support dropping 1 function".to_string(),
            None.into(),
        )
        .into());
    }
    let func_desc = func_desc.remove(0);

    let session = handler_args.session;
    let db_name = session.database();
    let (schema_name, function_name) =
        Binder::resolve_schema_qualified_name(db_name, func_desc.name)?;
    let search_path = session.config().get_search_path();
    let user_name = &session.auth_context().user_name;
    let schema_path = SchemaPath::new(schema_name.as_deref(), &search_path, user_name);

    let arg_types = match func_desc.args {
        Some(args) => {
            let mut arg_types = vec![];
            for arg in args {
                arg_types.push(bind_data_type(&arg.data_type)?);
            }
            Some(arg_types)
        }
        None => None,
    };

    let function_id = {
        let reader = session.env().catalog_reader().read_guard();
        let res = match arg_types {
            Some(arg_types) => {
                reader.get_function_by_name_args(db_name, schema_path, &function_name, &arg_types)
            }
            // check if there is only one function if arguments are not specified
            None => match reader.get_functions_by_name(db_name, schema_path, &function_name) {
                Ok((functions, schema_name)) => {
                    if functions.len() > 1 {
                        return Err(ErrorCode::CatalogError(format!("function name {function_name:?} is not unique\nHINT: Specify the argument list to select the function unambiguously.").into()).into());
                    }
                    Ok((
                        functions.into_iter().next().expect("no functions"),
                        schema_name,
                    ))
                }
                Err(e) => Err(e),
            },
        };
        match res {
            Ok((function, schema_name)) => {
                session.check_privilege_for_drop_alter(schema_name, &**function)?;
                function.id
            }
            Err(CatalogError::NotFound(kind, _)) if kind == "function" && if_exists => {
                return Ok(RwPgResponse::builder(StatementType::DROP_FUNCTION)
                    .notice(format!(
                        "function \"{}\" does not exist, skipping",
                        function_name
                    ))
                    .into());
            }
            Err(e) => return Err(e.into()),
        }
    };

    let catalog_writer = session.env().catalog_writer();
    catalog_writer.drop_function(function_id).await?;

    Ok(PgResponse::empty_result(StatementType::DROP_FUNCTION))
}
