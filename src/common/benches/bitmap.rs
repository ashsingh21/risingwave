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

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use itertools::Itertools;
use risingwave_common::buffer::{Bitmap, BitmapIter};

fn bench_bitmap(c: &mut Criterion) {
    const CHUNK_SIZE: usize = 1024;
    let x = Bitmap::zeros(CHUNK_SIZE);
    let y = Bitmap::ones(CHUNK_SIZE);
    let i = 0x123;
    c.bench_function("zeros", |b| b.iter(|| Bitmap::zeros(CHUNK_SIZE)));
    c.bench_function("ones", |b| b.iter(|| Bitmap::ones(CHUNK_SIZE)));
    c.bench_function("get", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                black_box(x.is_set(i));
            }
        })
    });
    c.bench_function("get_1000", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                black_box(x.is_set(i));
            }
        })
    });
    c.bench_function("get_1000_000", |b| {
        b.iter(|| {
            for _ in 0..1_000_000 {
                black_box(x.is_set(i));
            }
        })
    });
    c.bench_function("and", |b| b.iter(|| &x & &y));
    c.bench_function("or", |b| b.iter(|| &x | &y));
    c.bench_function("not", |b| b.iter(|| !&x));
}

/// Bench ~10 million records
fn bench_bitmap_iter(c: &mut Criterion) {
    const CHUNK_SIZE: usize = 1024;
    const N_CHUNKS: usize = 10_000;
    fn make_iterators(bitmaps: &[Bitmap]) -> Vec<BitmapIter<'_>> {
        bitmaps.iter().map(|bitmap| bitmap.iter()).collect_vec()
    }
    fn bench_bitmap_iter_inner(bench_id: &str, bitmap: Bitmap, c: &mut Criterion) {
        let bitmaps = vec![bitmap; N_CHUNKS];
        let make_iters = || make_iterators(&bitmaps);
        c.bench_function(bench_id, |b| {
            b.iter_batched(
                make_iters,
                |iters| {
                    for iter in iters {
                        for bit_flag in iter {
                            black_box(bit_flag);
                        }
                    }
                },
                BatchSize::SmallInput,
            )
        });
    }
    let zeros = Bitmap::zeros(CHUNK_SIZE);
    bench_bitmap_iter_inner("zeros_iter", zeros, c);
    let ones = Bitmap::ones(CHUNK_SIZE);
    bench_bitmap_iter_inner("ones_iter", ones, c);
}

criterion_group!(benches, bench_bitmap, bench_bitmap_iter);
criterion_main!(benches);
