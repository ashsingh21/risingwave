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

use std::collections::HashMap;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use risingwave_common::config::{MAX_CONNECTION_WINDOW_SIZE, STREAM_WINDOW_SIZE};
use risingwave_common::util::addr::HostAddr;
use risingwave_pb::catalog::SinkType;
use risingwave_pb::connector_service::connector_service_client::ConnectorServiceClient;
use risingwave_pb::connector_service::sink_stream_request::{Request as SinkRequest, StartSink};
use risingwave_pb::connector_service::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Streaming};

use crate::error::{Result, RpcError};
use crate::RpcClient;

#[derive(Clone, Debug)]
pub struct ConnectorClient(ConnectorServiceClient<Channel>);

impl ConnectorClient {
    pub async fn new(host_addr: HostAddr) -> Result<Self> {
        let channel = Endpoint::from_shared(format!("http://{}", &host_addr))
            .map_err(|e| {
                RpcError::Internal(anyhow!(format!(
                    "invalid connector endpoint `{}`: {:?}",
                    &host_addr, e
                )))
            })?
            .initial_connection_window_size(MAX_CONNECTION_WINDOW_SIZE)
            .initial_stream_window_size(STREAM_WINDOW_SIZE)
            .tcp_nodelay(true)
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await?;
        Ok(Self(ConnectorServiceClient::new(channel)))
    }

    /// Get source event stream
    pub async fn start_source_stream(
        &self,
        source_id: u64,
        source_type: SourceType,
        start_offset: Option<String>,
        properties: HashMap<String, String>,
        snapshot_done: bool,
    ) -> Result<Streaming<GetEventStreamResponse>> {
        tracing::info!(
            "start cdc source properties: {:?}, snapshot_done: {}",
            properties,
            snapshot_done
        );
        Ok(self
            .0
            .to_owned()
            .get_event_stream(GetEventStreamRequest {
                source_id,
                source_type: source_type as _,
                start_offset: start_offset.unwrap_or_default(),
                properties,
                snapshot_done,
            })
            .await
            .inspect_err(|err| {
                tracing::error!(
                    "failed to start stream for CDC source {}: {}",
                    source_id,
                    err.message()
                )
            })?
            .into_inner())
    }

    /// Validate source properties
    pub async fn validate_source_properties(
        &self,
        source_id: u64,
        source_type: SourceType,
        properties: HashMap<String, String>,
        table_schema: Option<TableSchema>,
    ) -> Result<()> {
        let response = self
            .0
            .to_owned()
            .validate_source(ValidateSourceRequest {
                source_id,
                source_type: source_type as _,
                properties,
                table_schema,
            })
            .await
            .inspect_err(|err| {
                tracing::error!("failed to validate source#{}: {}", source_id, err.message())
            })?
            .into_inner();

        response.error.map_or(Ok(()), |err| {
            Err(RpcError::Internal(anyhow!(format!(
                "source cannot pass validation: {}",
                err.error_message
            ))))
        })
    }

    pub async fn start_sink_stream(
        &self,
        connector_type: String,
        sink_id: u64,
        properties: HashMap<String, String>,
        table_schema: Option<TableSchema>,
        sink_payload_format: SinkPayloadFormat,
    ) -> Result<(UnboundedSender<SinkStreamRequest>, Streaming<SinkResponse>)> {
        let (request_sender, request_receiver) = unbounded_channel::<SinkStreamRequest>();

        // Send initial request in case of the blocking receive call from creating streaming request
        request_sender
            .send(SinkStreamRequest {
                request: Some(SinkRequest::Start(StartSink {
                    format: sink_payload_format as i32,
                    sink_config: Some(SinkConfig {
                        connector_type,
                        properties,
                        table_schema,
                    }),
                    sink_id,
                })),
            })
            .map_err(|err| RpcError::Internal(anyhow!(err.to_string())))?;

        let response = self
            .0
            .to_owned()
            .sink_stream(Request::new(UnboundedReceiverStream::new(request_receiver)))
            .await
            .map_err(RpcError::GrpcStatus)?
            .into_inner();

        Ok((request_sender, response))
    }

    pub async fn validate_sink_properties(
        &self,
        connector_type: String,
        properties: HashMap<String, String>,
        table_schema: Option<TableSchema>,
        sink_type: SinkType,
    ) -> Result<()> {
        let response = self
            .0
            .to_owned()
            .validate_sink(ValidateSinkRequest {
                sink_config: Some(SinkConfig {
                    connector_type,
                    properties,
                    table_schema,
                }),
                sink_type: sink_type as i32,
            })
            .await
            .inspect_err(|err| {
                tracing::error!("failed to validate sink properties: {}", err.message())
            })?
            .into_inner();
        response.error.map_or_else(
            || Ok(()), // If there is no error message, return Ok here.
            |err| {
                Err(RpcError::Internal(anyhow!(format!(
                    "sink cannot pass validation: {}",
                    err.error_message
                ))))
            },
        )
    }
}

#[async_trait]
impl RpcClient for ConnectorClient {
    async fn new_client(host_addr: HostAddr) -> Result<Self> {
        Self::new(host_addr).await
    }
}
