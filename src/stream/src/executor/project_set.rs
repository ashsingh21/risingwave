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

use std::fmt::{Debug, Formatter};

use either::Either;
use futures::StreamExt;
use futures_async_stream::try_stream;
use risingwave_common::array::{Op, StreamChunk};
use risingwave_common::catalog::{Field, Schema};
use risingwave_common::types::{DataType, DatumRef};
use risingwave_common::util::chunk_coalesce::DataChunkBuilder;
use risingwave_common::util::iter_util::ZipEqFast;
use risingwave_expr::table_function::ProjectSetSelectItem;

use super::error::StreamExecutorError;
use super::{BoxedExecutor, Executor, ExecutorInfo, Message, PkIndices, PkIndicesRef};

impl ProjectSetExecutor {
    pub fn new(
        input: Box<dyn Executor>,
        pk_indices: PkIndices,
        select_list: Vec<ProjectSetSelectItem>,
        executor_id: u64,
        chunk_size: usize,
    ) -> Self {
        let mut fields = vec![Field::with_name(DataType::Int64, "projected_row_id")];
        fields.extend(
            select_list
                .iter()
                .map(|expr| Field::unnamed(expr.return_type())),
        );

        let info = ExecutorInfo {
            schema: Schema { fields },
            pk_indices,
            identity: format!("ProjectSet {:X}", executor_id),
        };
        Self {
            input,
            info,
            select_list,
            chunk_size,
        }
    }
}

/// `ProjectSetExecutor` projects data with the `expr`. The `expr` takes a chunk of data,
/// and returns a new data chunk. And then, `ProjectSetExecutor` will insert, delete
/// or update element into next operator according to the result of the expression.
pub struct ProjectSetExecutor {
    input: BoxedExecutor,
    info: ExecutorInfo,
    /// Expressions of the current project_section.
    select_list: Vec<ProjectSetSelectItem>,
    chunk_size: usize,
}

impl Debug for ProjectSetExecutor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectSetExecutor")
            .field("exprs", &self.select_list)
            .finish()
    }
}

impl Executor for ProjectSetExecutor {
    fn execute(self: Box<Self>) -> super::BoxedMessageStream {
        self.execute_inner().boxed()
    }

    fn schema(&self) -> &Schema {
        &self.info.schema
    }

    fn pk_indices(&self) -> PkIndicesRef<'_> {
        &self.info.pk_indices
    }

    fn identity(&self) -> &str {
        &self.info.identity
    }
}

impl ProjectSetExecutor {
    #[try_stream(ok = Message, error = StreamExecutorError)]
    async fn execute_inner(self) {
        assert!(!self.select_list.is_empty());

        // First column will be `projected_row_id`, which represents the index in the
        // output table
        let mut ops_builder = Vec::with_capacity(self.chunk_size);
        let mut builder = DataChunkBuilder::new(
            std::iter::once(DataType::Int64)
                .chain(self.select_list.iter().map(|i| i.return_type()))
                .collect(),
            self.chunk_size,
        );
        // a temporary row buffer
        let mut row = vec![None as DatumRef<'_>; builder.num_columns()];

        #[for_await]
        for msg in self.input.execute() {
            match msg? {
                Message::Watermark(_) => {
                    // TODO: https://github.com/risingwavelabs/risingwave/issues/6042
                }
                m @ Message::Barrier(_) => yield m,
                Message::Chunk(chunk) => {
                    let mut results = Vec::with_capacity(self.select_list.len());
                    for select_item in &self.select_list {
                        let result = select_item.eval(chunk.data_chunk()).await?;
                        results.push(result);
                    }

                    // for each input row
                    for row_idx in 0..chunk.capacity() {
                        // ProjectSet cannot preserve that U- is followed by U+,
                        // so we rewrite update to insert/delete.
                        let op = match chunk.ops()[row_idx] {
                            Op::Delete | Op::UpdateDelete => Op::Delete,
                            Op::Insert | Op::UpdateInsert => Op::Insert,
                        };
                        // for each output row
                        for projected_row_id in 0i64.. {
                            // SAFETY:
                            // We use `row` as a buffer and don't read elements from the previous
                            // loop. The `transmute` is used for bypassing the borrow checker.
                            let row: &mut [DatumRef<'_>] =
                                unsafe { std::mem::transmute(row.as_mut_slice()) };

                            row[0] = Some(projected_row_id.into());
                            // if any of the set columns has a value
                            let mut valid = false;
                            // for each column
                            for (item, value) in results.iter_mut().zip_eq_fast(&mut row[1..]) {
                                *value = match item {
                                    Either::Left(state) => if let Some((i, value)) = state.peek() && i == row_idx {
                                        valid = true;
                                        value
                                    } else {
                                        None
                                    }
                                    Either::Right(array) => array.value_at(row_idx),
                                };
                            }
                            if !valid {
                                // no more output rows for the input row
                                break;
                            }
                            ops_builder.push(op);
                            if let Some(chunk) = builder.append_one_row(&*row) {
                                let ops = std::mem::replace(
                                    &mut ops_builder,
                                    Vec::with_capacity(self.chunk_size),
                                );
                                yield StreamChunk::from_parts(ops, chunk).into();
                            }
                            // move to the next row
                            for item in &mut results {
                                if let Either::Left(state) = item && matches!(state.peek(), Some((i, _)) if i == row_idx) {
                                    state.next().await?;
                                }
                            }
                        }
                    }
                    if let Some(chunk) = builder.consume_all() {
                        let ops = std::mem::replace(
                            &mut ops_builder,
                            Vec::with_capacity(self.chunk_size),
                        );
                        yield StreamChunk::from_parts(ops, chunk).into();
                    }
                }
            }
        }
    }
}
