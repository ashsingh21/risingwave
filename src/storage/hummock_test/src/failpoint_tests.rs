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

use std::ops::Bound;
use std::sync::Arc;

use bytes::{BufMut, Bytes};
use risingwave_common::cache::CachePriority;
use risingwave_common::catalog::TableId;
use risingwave_hummock_sdk::key::TABLE_PREFIX_LEN;
use risingwave_hummock_sdk::HummockReadEpoch;
use risingwave_meta::hummock::test_utils::setup_compute_env;
use risingwave_meta::hummock::MockHummockMetaClient;
use risingwave_rpc_client::HummockMetaClient;
use risingwave_storage::hummock::iterator::test_utils::mock_sstable_store;
use risingwave_storage::hummock::test_utils::{count_stream, default_opts_for_test};
use risingwave_storage::hummock::{CachePolicy, HummockStorage};
use risingwave_storage::storage_value::StorageValue;
use risingwave_storage::store::{
    LocalStateStore, NewLocalOptions, PrefetchOptions, ReadOptions, StateStoreRead, WriteOptions,
};
use risingwave_storage::StateStore;

use crate::get_notification_client_for_test;
use crate::test_utils::TestIngestBatch;

#[tokio::test]
#[ignore]
#[cfg(all(test, feature = "failpoints"))]
async fn test_failpoints_state_store_read_upload() {
    let mem_upload_err = "mem_upload_err";
    let mem_read_err = "mem_read_err";
    let sstable_store = mock_sstable_store();
    let hummock_options = Arc::new(default_opts_for_test());
    let (env, hummock_manager_ref, _cluster_manager_ref, worker_node) =
        setup_compute_env(8080).await;
    let meta_client = Arc::new(MockHummockMetaClient::new(
        hummock_manager_ref.clone(),
        worker_node.id,
    ));

    let hummock_storage = HummockStorage::for_test(
        hummock_options,
        sstable_store.clone(),
        meta_client.clone(),
        get_notification_client_for_test(env, hummock_manager_ref, worker_node),
    )
    .await
    .unwrap();

    let mut local = hummock_storage.new_local(NewLocalOptions::default()).await;

    let anchor = Bytes::from("aa");
    let mut batch1 = vec![
        (anchor.clone(), StorageValue::new_put("111")),
        (Bytes::from("cc"), StorageValue::new_put("222")),
    ];
    batch1.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

    let mut batch2 = vec![
        (Bytes::from("cc"), StorageValue::new_put("333")),
        (anchor.clone(), StorageValue::new_delete()),
    ];
    // Make sure the batch is sorted.
    batch2.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
    local.init(1);
    local
        .ingest_batch(
            batch1,
            vec![],
            WriteOptions {
                epoch: 1,
                table_id: Default::default(),
            },
        )
        .await
        .unwrap();

    local.seal_current_epoch(3);

    // Get the value after flushing to remote.
    let anchor_prefix_hint = {
        let mut ret = Vec::with_capacity(TABLE_PREFIX_LEN + anchor.len());
        ret.put_u32(TableId::default().table_id());
        ret.put_slice(anchor.as_ref());
        ret
    };
    let value = hummock_storage
        .get(
            anchor.clone(),
            1,
            ReadOptions {
                ignore_range_tombstone: false,
                prefix_hint: Some(Bytes::from(anchor_prefix_hint)),
                table_id: Default::default(),
                retention_seconds: None,
                read_version_from_backup: false,
                prefetch_options: Default::default(),
                cache_policy: CachePolicy::Fill(CachePriority::High),
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(value, Bytes::from("111"));
    // // Write second batch.
    local
        .ingest_batch(
            batch2,
            vec![],
            WriteOptions {
                epoch: 3,
                table_id: Default::default(),
            },
        )
        .await
        .unwrap();

    local.seal_current_epoch(u64::MAX);

    // sync epoch1 test the read_error
    let ssts = hummock_storage
        .seal_and_sync_epoch(1)
        .await
        .unwrap()
        .uncommitted_ssts;
    meta_client.commit_epoch(1, ssts).await.unwrap();
    hummock_storage
        .try_wait_epoch(HummockReadEpoch::Committed(1))
        .await
        .unwrap();
    // clear block cache
    sstable_store.clear_block_cache();
    sstable_store.clear_meta_cache();
    fail::cfg(mem_read_err, "return").unwrap();

    let anchor_prefix_hint = {
        let mut ret = Vec::with_capacity(TABLE_PREFIX_LEN + anchor.len());
        ret.put_u32(TableId::default().table_id());
        ret.put_slice(anchor.as_ref());
        ret
    };
    let result = hummock_storage
        .get(
            anchor.clone(),
            2,
            ReadOptions {
                ignore_range_tombstone: false,
                prefix_hint: Some(Bytes::from(anchor_prefix_hint)),
                table_id: Default::default(),
                retention_seconds: None,
                read_version_from_backup: false,
                prefetch_options: Default::default(),
                cache_policy: CachePolicy::Fill(CachePriority::High),
            },
        )
        .await;
    assert!(result.is_err());
    let result = hummock_storage
        .iter(
            (Bound::Unbounded, Bound::Included(Bytes::from("ee"))),
            2,
            ReadOptions {
                ignore_range_tombstone: false,
                prefix_hint: None,
                table_id: Default::default(),
                retention_seconds: None,
                read_version_from_backup: false,
                prefetch_options: Default::default(),
                cache_policy: CachePolicy::Fill(CachePriority::High),
            },
        )
        .await;
    assert!(result.is_err());

    let bee_prefix_hint = {
        let mut ret = Vec::with_capacity(TABLE_PREFIX_LEN + b"ee".as_ref().len());
        ret.put_u32(TableId::default().table_id());
        ret.put_slice(b"ee".as_ref().as_ref());
        ret
    };
    let value = hummock_storage
        .get(
            Bytes::from("ee"),
            2,
            ReadOptions {
                ignore_range_tombstone: false,
                prefix_hint: Some(Bytes::from(bee_prefix_hint)),
                table_id: Default::default(),
                retention_seconds: None,
                read_version_from_backup: false,
                prefetch_options: Default::default(),
                cache_policy: CachePolicy::Fill(CachePriority::High),
            },
        )
        .await
        .unwrap();
    assert!(value.is_none());
    fail::remove(mem_read_err);
    // test the upload_error
    fail::cfg(mem_upload_err, "return").unwrap();

    let result = hummock_storage.seal_and_sync_epoch(3).await;
    assert!(result.is_err());
    fail::remove(mem_upload_err);

    let ssts = hummock_storage
        .seal_and_sync_epoch(3)
        .await
        .unwrap()
        .uncommitted_ssts;
    meta_client.commit_epoch(3, ssts).await.unwrap();
    hummock_storage
        .try_wait_epoch(HummockReadEpoch::Committed(3))
        .await
        .unwrap();

    let anchor_prefix_hint = {
        let mut ret = Vec::with_capacity(TABLE_PREFIX_LEN + anchor.len());
        ret.put_u32(TableId::default().table_id());
        ret.put_slice(anchor.as_ref());
        ret
    };
    let value = hummock_storage
        .get(
            anchor.clone(),
            5,
            ReadOptions {
                ignore_range_tombstone: false,
                prefix_hint: Some(Bytes::from(anchor_prefix_hint)),
                table_id: Default::default(),
                retention_seconds: None,
                read_version_from_backup: false,
                prefetch_options: Default::default(),
                cache_policy: CachePolicy::Fill(CachePriority::High),
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(value, Bytes::from("111"));
    let iters = hummock_storage
        .iter(
            (Bound::Unbounded, Bound::Included(Bytes::from("ee"))),
            5,
            ReadOptions {
                ignore_range_tombstone: false,
                prefix_hint: None,
                table_id: Default::default(),
                retention_seconds: None,
                read_version_from_backup: false,
                prefetch_options: PrefetchOptions::new_for_exhaust_iter(),
                cache_policy: CachePolicy::Fill(CachePriority::High),
            },
        )
        .await
        .unwrap();
    let len = count_stream(iters).await;
    assert_eq!(len, 2);
}
