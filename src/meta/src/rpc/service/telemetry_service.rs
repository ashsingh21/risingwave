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

use std::ops::Deref;
use std::sync::Arc;

use risingwave_pb::meta::telemetry_info_service_server::TelemetryInfoService;
use risingwave_pb::meta::{GetTelemetryInfoRequest, TelemetryInfoResponse};
use tonic::{Request, Response, Status};

use crate::model::ClusterId;
use crate::storage::MetaStore;

pub struct TelemetryInfoServiceImpl<S: MetaStore> {
    meta_store: Arc<S>,
}

impl<S: MetaStore> TelemetryInfoServiceImpl<S> {
    pub fn new(meta_store: Arc<S>) -> Self {
        Self { meta_store }
    }

    async fn get_tracking_id(&self) -> Option<ClusterId> {
        ClusterId::from_meta_store(self.meta_store.deref())
            .await
            .ok()
            .flatten()
    }
}

#[async_trait::async_trait]
impl<S: MetaStore> TelemetryInfoService for TelemetryInfoServiceImpl<S> {
    async fn get_telemetry_info(
        &self,
        _request: Request<GetTelemetryInfoRequest>,
    ) -> Result<Response<TelemetryInfoResponse>, Status> {
        match self.get_tracking_id().await {
            Some(tracking_id) => Ok(Response::new(TelemetryInfoResponse {
                tracking_id: Some(tracking_id.into()),
            })),
            None => Ok(Response::new(TelemetryInfoResponse { tracking_id: None })),
        }
    }
}
