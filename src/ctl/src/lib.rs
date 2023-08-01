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

#![feature(let_chains)]
#![feature(hash_drain_filter)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use cmd_impl::bench::BenchCommands;
use cmd_impl::hummock::SstDumpArgs;
use risingwave_pb::meta::update_worker_node_schedulability_request::Schedulability;

use crate::cmd_impl::hummock::{
    build_compaction_config_vec, list_pinned_snapshots, list_pinned_versions,
};
use crate::common::CtlContext;

pub mod cmd_impl;
pub mod common;

/// risectl provides internal access to the RisingWave cluster. Generally, you will need
/// to provide the meta address and the state store URL to enable risectl to access the cluster. You
/// must start RisingWave in full cluster mode (e.g. enable MinIO and compactor in risedev.yml)
/// instead of playground mode to use this tool. risectl will read environment variables
/// `RW_META_ADDR` and `RW_HUMMOCK_URL` to configure itself.
#[derive(Parser)]
#[clap(version, about = "The DevOps tool that provides internal access to the RisingWave cluster", long_about = None)]
#[clap(propagate_version = true)]
#[clap(infer_subcommands = true)]
pub struct CliOpts {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[clap(infer_subcommands = true)]
enum Commands {
    /// Commands for Compute
    #[clap(subcommand)]
    Compute(ComputeCommands),
    /// Commands for Hummock
    #[clap(subcommand)]
    Hummock(HummockCommands),
    /// Commands for Tables
    #[clap(subcommand)]
    Table(TableCommands),
    /// Commands for Meta
    #[clap(subcommand)]
    Meta(MetaCommands),
    /// Commands for Scaling
    #[clap(subcommand)]
    Scale(ScaleCommands),
    /// Commands for Benchmarks
    #[clap(subcommand)]
    Bench(BenchCommands),
    /// Commands for tracing the compute nodes
    Trace,
    // TODO(yuhao): profile other nodes
    /// Commands for profilng the compute nodes
    Profile {
        #[clap(short, long = "sleep")]
        sleep: u64,
    },
}

#[derive(Subcommand)]
enum ComputeCommands {
    /// Show all the configuration parameters on compute node
    ShowConfig { host: String },
}

#[derive(Subcommand)]
enum HummockCommands {
    /// list latest Hummock version on meta node
    ListVersion {
        #[clap(short, long = "verbose", default_value_t = false)]
        verbose: bool,
    },

    /// list hummock version deltas in the meta store
    ListVersionDeltas {
        #[clap(short, long = "start-version-delta-id", default_value_t = 0)]
        start_id: u64,

        #[clap(short, long = "num-epochs", default_value_t = 100)]
        num_epochs: u32,
    },
    /// Forbid hummock commit new epochs, which is a prerequisite for compaction deterministic test
    DisableCommitEpoch,
    /// list all Hummock key-value pairs
    ListKv {
        #[clap(short, long = "epoch", default_value_t = u64::MAX)]
        epoch: u64,

        #[clap(short, long = "table-id")]
        table_id: u32,

        // data directory for hummock state store. None: use default
        data_dir: Option<String>,
    },
    SstDump(SstDumpArgs),
    /// trigger a targeted compaction through compaction_group_id
    TriggerManualCompaction {
        #[clap(short, long = "compaction-group-id", default_value_t = 2)]
        compaction_group_id: u64,

        #[clap(short, long = "table-id", default_value_t = 0)]
        table_id: u32,

        #[clap(short, long = "level", default_value_t = 1)]
        level: u32,
    },
    /// trigger a full GC for SSTs that is not in version and with timestamp <= now -
    /// sst_retention_time_sec.
    TriggerFullGc {
        #[clap(short, long = "sst_retention_time_sec", default_value_t = 259200)]
        sst_retention_time_sec: u64,
    },
    /// List pinned versions of each worker.
    ListPinnedVersions {},
    /// List pinned snapshots of each worker.
    ListPinnedSnapshots {},
    /// List all compaction groups.
    ListCompactionGroup,
    /// Update compaction config for compaction groups.
    UpdateCompactionConfig {
        #[clap(long)]
        compaction_group_ids: Vec<u64>,
        #[clap(long)]
        max_bytes_for_level_base: Option<u64>,
        #[clap(long)]
        max_bytes_for_level_multiplier: Option<u64>,
        #[clap(long)]
        max_compaction_bytes: Option<u64>,
        #[clap(long)]
        sub_level_max_compaction_bytes: Option<u64>,
        #[clap(long)]
        level0_tier_compact_file_number: Option<u64>,
        #[clap(long)]
        target_file_size_base: Option<u64>,
        #[clap(long)]
        compaction_filter_mask: Option<u32>,
        #[clap(long)]
        max_sub_compaction: Option<u32>,
        #[clap(long)]
        level0_stop_write_threshold_sub_level_number: Option<u64>,
        #[clap(long)]
        level0_sub_level_compact_level_count: Option<u32>,
    },
    /// Split given compaction group into two. Moves the given tables to the new group.
    SplitCompactionGroup {
        #[clap(long)]
        compaction_group_id: u64,
        #[clap(long)]
        table_ids: Vec<u32>,
    },
    /// Pause version checkpoint, which subsequently pauses GC of delta log and SST object.
    PauseVersionCheckpoint,
    /// Resume version checkpoint, which subsequently resumes GC of delta log and SST object.
    ResumeVersionCheckpoint,
    /// Replay version from the checkpoint one to the latest one.
    ReplayVersion,
    /// List compaction status
    ListCompactionStatus {
        #[clap(short, long = "verbose", default_value_t = false)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
enum TableCommands {
    /// scan a state table with MV name
    Scan {
        /// name of the materialized view to operate on
        mv_name: String,
        // data directory for hummock state store. None: use default
        data_dir: Option<String>,
    },
    /// scan a state table using Id
    ScanById {
        /// id of the state table to operate on
        table_id: u32,
        // data directory for hummock state store. None: use default
        data_dir: Option<String>,
    },
    /// list all state tables
    List,
}

#[derive(clap::Args, Debug)]
#[clap(group(clap::ArgGroup::new("workers_group").required(true).multiple(true).args(&["include_workers", "exclude_workers"])))]
pub struct ScaleResizeCommands {
    /// The worker that needs to be excluded during scheduling, worker_id and worker_host are both
    /// supported
    #[clap(
        long,
        value_delimiter = ',',
        value_name = "worker_id or worker_host, ..."
    )]
    exclude_workers: Option<Vec<String>>,

    /// The worker that needs to be included during scheduling, worker_id and worker_host are both
    /// supported
    #[clap(
        long,
        value_delimiter = ',',
        value_name = "worker_id or worker_host, ..."
    )]
    include_workers: Option<Vec<String>>,

    /// Will generate a plan supported by the `reschedule` command and save it to the provided path
    /// by the `--output`.
    #[clap(long, default_value_t = false)]
    generate: bool,

    /// The output file to write the generated plan to, standard output by default
    #[clap(long)]
    output: Option<String>,

    /// Automatic yes to prompts
    #[clap(short = 'y', long, default_value_t = false)]
    yes: bool,

    /// Specify the fragment ids that need to be scheduled.
    /// empty by default, which means all fragments will be scheduled
    #[clap(long)]
    fragments: Option<Vec<u32>>,
}

#[derive(Subcommand, Debug)]
enum ScaleCommands {
    /// The resize command scales the cluster by specifying the workers to be included and
    /// excluded.
    Resize(ScaleResizeCommands),
    /// mark a compute node as unschedulable
    #[clap(verbatim_doc_comment)]
    Cordon {
        /// Workers that need to be cordoned, both id and host are supported.
        #[clap(
            long,
            required = true,
            value_delimiter = ',',
            value_name = "id or host,..."
        )]
        workers: Vec<String>,
    },
    /// mark a compute node as schedulable. Nodes are schedulable unless they are cordoned
    Uncordon {
        /// Workers that need to be uncordoned, both id and host are supported.
        #[clap(
            long,
            required = true,
            value_delimiter = ',',
            value_name = "id or host,..."
        )]
        workers: Vec<String>,
    },
}

#[derive(Subcommand)]
enum MetaCommands {
    /// pause the stream graph
    Pause,
    /// resume the stream graph
    Resume,
    /// get cluster info
    ClusterInfo,
    /// get source split info
    SourceSplitInfo,
    /// Reschedule the parallel unit in the stream graph
    ///
    /// The format is `fragment_id-[removed]+[added]`
    /// You can provide either `removed` only or `added` only, but `removed` should be preceded by
    /// `added` when both are provided.
    ///
    /// For example, for plan `100-[1,2,3]+[4,5]` the follow request will be generated:
    /// {
    ///     100: Reschedule {
    ///         added_parallel_units: [4,5],
    ///         removed_parallel_units: [1,2,3],
    ///     }
    /// }
    /// Use ; to separate multiple fragment
    #[clap(verbatim_doc_comment)]
    #[clap(group(clap::ArgGroup::new("input_group").required(true).args(&["plan", "from"])))]
    Reschedule {
        /// Plan of reschedule, needs to be used with `revision`
        #[clap(long, requires = "revision")]
        plan: Option<String>,
        /// Revision of the plan
        #[clap(long)]
        revision: Option<u64>,
        /// Reschedule from a specific file
        #[clap(long, conflicts_with = "revision", value_hint = clap::ValueHint::AnyPath)]
        from: Option<String>,
        /// Show the plan only, no actual operation
        #[clap(long, default_value = "false")]
        dry_run: bool,
    },
    /// backup meta by taking a meta snapshot
    BackupMeta,
    /// delete meta snapshots
    DeleteMetaSnapshots { snapshot_ids: Vec<u64> },

    /// List all existing connections in the catalog
    ListConnections,

    /// List fragment to parallel units mapping for serving
    ListServingFragmentMapping,

    /// Unregister workers from the cluster
    UnregisterWorkers {
        /// The workers that needs to be unregistered, worker_id and worker_host are both supported
        #[clap(
            long,
            required = true,
            value_delimiter = ',',
            value_name = "worker_id or worker_host, ..."
        )]
        workers: Vec<String>,

        /// Automatic yes to prompts
        #[clap(short = 'y', long, default_value_t = false)]
        yes: bool,

        /// The worker not found will be ignored
        #[clap(long, default_value_t = false)]
        ignore_not_found: bool,
    },
}

pub async fn start(opts: CliOpts) -> Result<()> {
    let context = CtlContext::default();
    let result = start_impl(opts, &context).await;
    context.try_close().await;
    result
}

pub async fn start_impl(opts: CliOpts, context: &CtlContext) -> Result<()> {
    match opts.command {
        Commands::Compute(ComputeCommands::ShowConfig { host }) => {
            cmd_impl::compute::show_config(&host).await?
        }
        Commands::Hummock(HummockCommands::DisableCommitEpoch) => {
            cmd_impl::hummock::disable_commit_epoch(context).await?
        }
        Commands::Hummock(HummockCommands::ListVersion { verbose }) => {
            cmd_impl::hummock::list_version(context, verbose).await?;
        }
        Commands::Hummock(HummockCommands::ListVersionDeltas {
            start_id,
            num_epochs,
        }) => {
            cmd_impl::hummock::list_version_deltas(context, start_id, num_epochs).await?;
        }
        Commands::Hummock(HummockCommands::ListKv {
            epoch,
            table_id,
            data_dir,
        }) => {
            cmd_impl::hummock::list_kv(context, epoch, table_id, data_dir).await?;
        }
        Commands::Hummock(HummockCommands::SstDump(args)) => {
            cmd_impl::hummock::sst_dump(context, args).await.unwrap()
        }
        Commands::Hummock(HummockCommands::TriggerManualCompaction {
            compaction_group_id,
            table_id,
            level,
        }) => {
            cmd_impl::hummock::trigger_manual_compaction(
                context,
                compaction_group_id,
                table_id,
                level,
            )
            .await?
        }
        Commands::Hummock(HummockCommands::TriggerFullGc {
            sst_retention_time_sec,
        }) => cmd_impl::hummock::trigger_full_gc(context, sst_retention_time_sec).await?,
        Commands::Hummock(HummockCommands::ListPinnedVersions {}) => {
            list_pinned_versions(context).await?
        }
        Commands::Hummock(HummockCommands::ListPinnedSnapshots {}) => {
            list_pinned_snapshots(context).await?
        }
        Commands::Hummock(HummockCommands::ListCompactionGroup) => {
            cmd_impl::hummock::list_compaction_group(context).await?
        }
        Commands::Hummock(HummockCommands::UpdateCompactionConfig {
            compaction_group_ids,
            max_bytes_for_level_base,
            max_bytes_for_level_multiplier,
            max_compaction_bytes,
            sub_level_max_compaction_bytes,
            level0_tier_compact_file_number,
            target_file_size_base,
            compaction_filter_mask,
            max_sub_compaction,
            level0_stop_write_threshold_sub_level_number,
            level0_sub_level_compact_level_count,
        }) => {
            cmd_impl::hummock::update_compaction_config(
                context,
                compaction_group_ids,
                build_compaction_config_vec(
                    max_bytes_for_level_base,
                    max_bytes_for_level_multiplier,
                    max_compaction_bytes,
                    sub_level_max_compaction_bytes,
                    level0_tier_compact_file_number,
                    target_file_size_base,
                    compaction_filter_mask,
                    max_sub_compaction,
                    level0_stop_write_threshold_sub_level_number,
                    level0_sub_level_compact_level_count,
                ),
            )
            .await?
        }
        Commands::Hummock(HummockCommands::SplitCompactionGroup {
            compaction_group_id,
            table_ids,
        }) => {
            cmd_impl::hummock::split_compaction_group(context, compaction_group_id, &table_ids)
                .await?;
        }
        Commands::Hummock(HummockCommands::PauseVersionCheckpoint) => {
            cmd_impl::hummock::pause_version_checkpoint(context).await?;
        }
        Commands::Hummock(HummockCommands::ResumeVersionCheckpoint) => {
            cmd_impl::hummock::resume_version_checkpoint(context).await?;
        }
        Commands::Hummock(HummockCommands::ReplayVersion) => {
            cmd_impl::hummock::replay_version(context).await?;
        }
        Commands::Hummock(HummockCommands::ListCompactionStatus { verbose }) => {
            cmd_impl::hummock::list_compaction_status(context, verbose).await?;
        }
        Commands::Table(TableCommands::Scan { mv_name, data_dir }) => {
            cmd_impl::table::scan(context, mv_name, data_dir).await?
        }
        Commands::Table(TableCommands::ScanById { table_id, data_dir }) => {
            cmd_impl::table::scan_id(context, table_id, data_dir).await?
        }
        Commands::Table(TableCommands::List) => cmd_impl::table::list(context).await?,
        Commands::Bench(cmd) => cmd_impl::bench::do_bench(context, cmd).await?,
        Commands::Meta(MetaCommands::Pause) => cmd_impl::meta::pause(context).await?,
        Commands::Meta(MetaCommands::Resume) => cmd_impl::meta::resume(context).await?,
        Commands::Meta(MetaCommands::ClusterInfo) => cmd_impl::meta::cluster_info(context).await?,
        Commands::Meta(MetaCommands::SourceSplitInfo) => {
            cmd_impl::meta::source_split_info(context).await?
        }
        Commands::Meta(MetaCommands::Reschedule {
            from,
            dry_run,
            plan,
            revision,
        }) => cmd_impl::meta::reschedule(context, plan, revision, from, dry_run).await?,
        Commands::Meta(MetaCommands::BackupMeta) => cmd_impl::meta::backup_meta(context).await?,
        Commands::Meta(MetaCommands::DeleteMetaSnapshots { snapshot_ids }) => {
            cmd_impl::meta::delete_meta_snapshots(context, &snapshot_ids).await?
        }
        Commands::Meta(MetaCommands::ListConnections) => {
            cmd_impl::meta::list_connections(context).await?
        }
        Commands::Meta(MetaCommands::ListServingFragmentMapping) => {
            cmd_impl::meta::list_serving_fragment_mappings(context).await?
        }
        Commands::Meta(MetaCommands::UnregisterWorkers {
            workers,
            yes,
            ignore_not_found,
        }) => cmd_impl::meta::unregister_workers(context, workers, yes, ignore_not_found).await?,
        Commands::Trace => cmd_impl::trace::trace(context).await?,
        Commands::Profile { sleep } => cmd_impl::profile::profile(context, sleep).await?,
        Commands::Scale(ScaleCommands::Resize(resize)) => {
            cmd_impl::scale::resize(context, resize).await?
        }
        Commands::Scale(ScaleCommands::Cordon { workers }) => {
            cmd_impl::scale::update_schedulability(context, workers, Schedulability::Unschedulable)
                .await?
        }
        Commands::Scale(ScaleCommands::Uncordon { workers }) => {
            cmd_impl::scale::update_schedulability(context, workers, Schedulability::Schedulable)
                .await?
        }
    }
    Ok(())
}
